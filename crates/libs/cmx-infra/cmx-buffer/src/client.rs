//! Redis 客户端封装模块
//!
//! 提供 `RedisClient` 结构体，使用 redis-rs 原生异步连接（MultiplexedConnection/ClusterConnection），
//! 无需外部连接池。连接 handle 可低成本 Clone，天然支持高并发。

use crate::config::{CacheConfig, LockConfig, RedisConfig, RedisMode};
use crate::error::{Error, Result};
use crate::logging::ConnLog;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

/// Redis 异步连接引用，统一封装单机和集群连接
///
/// 两种变体都实现了 `redis::aio::ConnectionLike` trait，
/// 通过为枚举实现该 trait，所有 `query_async` 调用无需关心底层连接类型。
#[derive(Clone)]
pub enum RedisConnectionRef {
    /// 单机模式 - 使用 ConnectionManager（自带断线重连）
    Standalone(redis::aio::ConnectionManager),
    /// 集群模式 - 使用 ClusterConnection（自带重连和路由）
    Cluster(redis::cluster_async::ClusterConnection),
    /// 测试用 Mock 连接，公开供集成测试使用
    Mock(crate::mock::MockConnection),
}

impl redis::aio::ConnectionLike for RedisConnectionRef {
    fn req_packed_command<'a>(
        &'a mut self,
        cmd: &'a redis::Cmd,
    ) -> redis::RedisFuture<'a, redis::Value> {
        match self {
            Self::Standalone(conn) => conn.req_packed_command(cmd),
            Self::Cluster(conn) => conn.req_packed_command(cmd),
            Self::Mock(backend) => backend.req_packed_command(cmd),
        }
    }

    fn req_packed_commands<'a>(
        &'a mut self,
        cmd: &'a redis::Pipeline,
        offset: usize,
        count: usize,
    ) -> redis::RedisFuture<'a, Vec<redis::Value>> {
        match self {
            Self::Standalone(conn) => conn.req_packed_commands(cmd, offset, count),
            Self::Cluster(conn) => conn.req_packed_commands(cmd, offset, count),
            Self::Mock(backend) => backend.req_packed_commands(cmd, offset, count),
        }
    }

    fn get_db(&self) -> i64 {
        match self {
            Self::Standalone(conn) => conn.get_db(),
            Self::Cluster(conn) => conn.get_db(),
            Self::Mock(backend) => backend.get_db(),
        }
    }
}

/// 订阅专用连接，独立于主连接
///
/// 单机模式：ConnectionManager + set_automatic_resubscription()
///   → 自动检测断线（Disconnection push）+ 自动重新订阅
/// 集群模式：ClusterConnection + 内置 SubscriptionTracker
///   → 需要心跳 PING 触发重连 + 自动重新订阅
pub enum SubscriberConnection {
    /// 单机模式 - ConnectionManager（自带断线重连 + 自动重新订阅）
    Standalone(redis::aio::ConnectionManager),
    /// 集群模式 - ClusterConnection（自带重连 + SubscriptionTracker 自动重新订阅）
    Cluster(redis::cluster_async::ClusterConnection),
}

impl SubscriberConnection {
    /// 订阅频道
    pub async fn subscribe(&mut self, channel: &str) -> Result<()> {
        match self {
            Self::Standalone(conn) => {
                conn.subscribe(channel).await?;
            }
            Self::Cluster(conn) => {
                conn.subscribe(channel).await?;
            }
        }
        Ok(())
    }

    /// 取消订阅频道
    pub async fn unsubscribe(&mut self, channel: &str) -> Result<()> {
        match self {
            Self::Standalone(conn) => {
                conn.unsubscribe(channel).await?;
            }
            Self::Cluster(conn) => {
                conn.unsubscribe(channel).await?;
            }
        }
        Ok(())
    }

    /// 模式订阅
    pub async fn psubscribe(&mut self, pattern: &str) -> Result<()> {
        match self {
            Self::Standalone(conn) => {
                conn.psubscribe(pattern).await?;
            }
            Self::Cluster(conn) => {
                conn.psubscribe(pattern).await?;
            }
        }
        Ok(())
    }

    /// 取消模式订阅
    pub async fn punsubscribe(&mut self, pattern: &str) -> Result<()> {
        match self {
            Self::Standalone(conn) => {
                conn.punsubscribe(pattern).await?;
            }
            Self::Cluster(conn) => {
                conn.punsubscribe(pattern).await?;
            }
        }
        Ok(())
    }
}

/// Redis 客户端包装器
///
/// 使用 redis-rs 原生异步连接，无需外部连接池。
/// `MultiplexedConnection` 线程安全且可低成本克隆，天然支持高并发。
#[derive(Clone)]
pub struct RedisClient {
    connection: RedisConnectionRef,
    config: RedisConfig,
    cache_config: CacheConfig,
    lock_config: LockConfig,
}

impl RedisClient {
    /// 从配置创建新的 Redis 客户端（使用默认缓存和锁配置）
    pub async fn new(config: RedisConfig) -> Result<Self> {
        let cache_config = CacheConfig::new();
        let lock_config = LockConfig::new();
        Self::new_with_configs(config, cache_config, lock_config).await
    }

    /// 从配置创建新的 Redis 客户端（带缓存和锁配置）
    pub async fn new_with_configs(
        config: RedisConfig,
        cache_config: CacheConfig,
        lock_config: LockConfig,
    ) -> Result<Self> {
        let connection = Self::create_connection(&config).await?;

        Ok(Self {
            connection,
            config,
            cache_config,
            lock_config,
        })
    }

    /// 用已有连接和配置创建 Redis 客户端（仅供测试使用）
    ///
    /// 允许注入 Mock 连接，用于在不依赖真实 Redis 的情况下测试上层逻辑。
    pub fn new_with_connection(
        connection: RedisConnectionRef,
        config: RedisConfig,
        cache_config: CacheConfig,
        lock_config: LockConfig,
    ) -> Self {
        Self {
            connection,
            config,
            cache_config,
            lock_config,
        }
    }

    /// 根据配置创建对应的连接
    async fn create_connection(config: &RedisConfig) -> Result<RedisConnectionRef> {
        match config.mode {
            RedisMode::Standalone => {
                info!(url = %config.url, "创建 Redis 单机连接（ConnectionManager）");
                let client = redis::Client::open(config.url.as_str())
                    .map_err(|e| Error::ConnectionError(e.to_string()))?;
                let conn_manager = client
                    .get_connection_manager()
                    .await
                    .map_err(|e| Error::ConnectionError(e.to_string()))?;
                ConnLog::connected(&config.url);
                Ok(RedisConnectionRef::Standalone(conn_manager))
            }
            RedisMode::Cluster => {
                let urls = if config.cluster_urls.is_empty() {
                    return Err(Error::ConfigError(
                        "集群模式需要至少一个节点地址 (cluster_urls)".to_string(),
                    ));
                } else {
                    &config.cluster_urls
                };
                info!(urls = ?urls, "创建 Redis 集群连接（ClusterConnection）");
                let urls_str: Vec<&str> = urls.iter().map(|s| s.as_str()).collect();
                let cluster_client = redis::cluster::ClusterClient::new(urls_str)
                    .map_err(|e| Error::ConnectionError(e.to_string()))?;
                let cluster_conn = cluster_client
                    .get_async_connection()
                    .await
                    .map_err(|e| Error::ConnectionError(e.to_string()))?;
                ConnLog::connected(&format!("cluster://{:?}", urls));
                Ok(RedisConnectionRef::Cluster(cluster_conn))
            }
        }
    }

    /// 获取连接引用（clone，无需 await）
    ///
    /// `MultiplexedConnection` 和 `ClusterConnection` 都是可低成本克隆的，
    /// 克隆后共享底层 TCP 连接，天然支持并发使用。
    pub fn get_connection(&self) -> RedisConnectionRef {
        self.connection.clone()
    }

    /// 获取配置
    pub fn config(&self) -> &RedisConfig {
        &self.config
    }

    /// 获取缓存配置
    pub fn cache_config(&self) -> &CacheConfig {
        &self.cache_config
    }

    /// 获取锁配置
    pub fn lock_config(&self) -> &LockConfig {
        &self.lock_config
    }

    /// 获取键前缀
    pub fn key_prefix(&self) -> &str {
        &self.config.key_prefix
    }

    /// 组合键名
    pub fn build_key(&self, key: &str) -> String {
        if self.cache_config.enable_prefix {
            format!("{}{}", self.config.key_prefix, key)
        } else {
            key.to_string()
        }
    }

    /// 检查连接是否有效
    pub async fn is_connected(&self) -> bool {
        let mut conn = self.get_connection();
        let result: std::result::Result<String, redis::RedisError> =
            redis::cmd("PING").query_async(&mut conn).await;
        result.is_ok()
    }

    /// 关闭连接
    pub async fn close(&self) -> Result<()> {
        info!("关闭 Redis 连接");
        Ok(())
    }

    /// 创建专用的 Pub/Sub 订阅连接（带 push_sender）
    ///
    /// 此连接独立于主连接，专用于订阅和接收推送消息。
    /// 使用 RESP3 协议 + push_sender 统一单机和集群的消息接收。
    /// 内置自动重连和自动重新订阅机制。
    pub async fn create_subscriber_connection(
        &self,
    ) -> Result<(
        SubscriberConnection,
        tokio::sync::mpsc::UnboundedReceiver<redis::PushInfo>,
    )> {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        match self.config.mode {
            RedisMode::Standalone => {
                info!("创建 Pub/Sub 订阅连接（单机 - ConnectionManager + 自动重新订阅）");
                let url = Self::ensure_resp3_url(&self.config.url);
                let client = redis::Client::open(url.as_str())
                    .map_err(|e| Error::ConnectionError(e.to_string()))?;
                let conn_manager = client
                    .get_connection_manager_with_config(
                        redis::aio::ConnectionManagerConfig::new()
                            .set_push_sender(tx)
                            .set_automatic_resubscription(),
                    )
                    .await
                    .map_err(|e| Error::ConnectionError(e.to_string()))?;
                Ok((SubscriberConnection::Standalone(conn_manager), rx))
            }
            RedisMode::Cluster => {
                if self.config.cluster_urls.is_empty() {
                    return Err(Error::ConfigError(
                        "集群模式需要至少一个节点地址 (cluster_urls)".to_string(),
                    ));
                }
                info!(
                    "创建 Pub/Sub 订阅连接（集群 - ClusterConnection + 内置 SubscriptionTracker）"
                );
                let urls: Vec<&str> = self
                    .config
                    .cluster_urls
                    .iter()
                    .map(|s| s.as_str())
                    .collect();
                let cluster_client = redis::cluster::ClusterClientBuilder::new(urls)
                    .use_protocol(redis::ProtocolVersion::RESP3)
                    .push_sender(tx)
                    .build()
                    .map_err(|e| Error::ConnectionError(e.to_string()))?;
                let cluster_conn = cluster_client
                    .get_async_connection()
                    .await
                    .map_err(|e| Error::ConnectionError(e.to_string()))?;
                Ok((SubscriberConnection::Cluster(cluster_conn), rx))
            }
        }
    }

    /// 确保 URL 包含 protocol=3 参数
    fn ensure_resp3_url(url: &str) -> String {
        if url.contains("protocol=3") {
            url.to_string()
        } else if url.contains('?') {
            format!("{}&protocol=3", url)
        } else {
            format!("{}?protocol=3", url)
        }
    }
}

/// 线程安全的 Redis 客户端
pub type SharedRedisClient = Arc<RwLock<RedisClient>>;

/// 全局 Redis 客户端单例
pub struct GlobalRedisClient;

static GLOBAL_REDIS_CLIENT: std::sync::OnceLock<SharedRedisClient> = std::sync::OnceLock::new();

impl GlobalRedisClient {
    /// 初始化全局 Redis 客户端
    pub async fn initialize(config: RedisConfig) -> Result<()> {
        let client = RedisClient::new(config).await?;
        GLOBAL_REDIS_CLIENT
            .set(Arc::new(RwLock::new(client)))
            .map_err(|_| Error::ConfigError("全局 Redis 客户端已初始化".to_string()))?;
        Ok(())
    }

    /// 获取全局 Redis 客户端
    pub fn get() -> &'static SharedRedisClient {
        GLOBAL_REDIS_CLIENT
            .get()
            .expect("全局 Redis 客户端未初始化，请先调用 GlobalRedisClient::initialize()")
    }

    /// 检查是否已初始化
    pub fn is_initialized() -> bool {
        GLOBAL_REDIS_CLIENT.get().is_some()
    }
}

/// 创建共享的 Redis 客户端
pub async fn create_shared_client(config: RedisConfig) -> Result<SharedRedisClient> {
    let client = RedisClient::new(config).await?;
    Ok(Arc::new(RwLock::new(client)))
}

/// 从共享客户端获取引用
pub async fn get_client(
    client: &SharedRedisClient,
) -> tokio::sync::RwLockReadGuard<'_, RedisClient> {
    client.read().await
}

/// 从共享客户端获取可变引用
pub async fn get_client_mut(
    client: &SharedRedisClient,
) -> tokio::sync::RwLockWriteGuard<'_, RedisClient> {
    client.write().await
}
