use crate::config::{CacheConfig, LockConfig, RedisConfig};
use crate::error::{Error, Result};
use crate::logging::ConnLog;
use redis::{aio::ConnectionManager, Client, RedisResult};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;
use std::sync::OnceLock;
use std::sync::Mutex;

/**
 * @Author: AI Assistant
 * @Date: 2026-03-16
 * @Describe: Redis 客户端封装
 */

/// Redis 客户端包装器
#[derive(Clone)]
pub struct RedisClient {
    inner: ConnectionManager,
    config: RedisConfig,
    cache_config: CacheConfig,
    lock_config: LockConfig,
}

impl RedisClient {
    /// 从配置创建新的 Redis 客户端
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
        info!(
            url = %config.url,
            pool_size = config.pool_size,
            "创建 Redis 客户端"
        );

        let client = Client::open(config.url.as_str())
            .map_err(|e| Error::ConnectionError(e.to_string()))?;

        let connection_manager = client
            .get_connection_manager()
            .await
            .map_err(|e| Error::ConnectionError(e.to_string()))?;

        ConnLog::connected(&config.url);

        Ok(Self {
            inner: connection_manager,
            config,
            cache_config,
            lock_config,
        })
    }

    /// 获取 Redis 连接管理器
    pub fn inner(&self) -> &ConnectionManager {
        &self.inner
    }

    /// 获取可变连接管理器引用
    pub fn inner_mut(&mut self) -> &mut ConnectionManager {
        &mut self.inner
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
        let mut conn = self.inner.clone();
        let result: RedisResult<String> = redis::cmd("PING")
            .query_async(&mut conn)
            .await;
        result.is_ok()
    }

    /// 关闭连接
    pub async fn close(&self) -> Result<()> {
        info!("关闭 Redis 连接");
        Ok(())
    }
}

/// 线程安全的 Redis 客户端
pub type SharedRedisClient = Arc<RwLock<RedisClient>>;

/// 创建共享的 Redis 客户端
pub async fn create_shared_client(config: RedisConfig) -> Result<SharedRedisClient> {
    let client = RedisClient::new(config).await?;
    Ok(Arc::new(RwLock::new(client)))
}

/// 从共享客户端获取引用
pub async fn get_client(client: &SharedRedisClient) -> tokio::sync::RwLockReadGuard<'_, RedisClient> {
    client.read().await
}

/// 从共享客户端获取可变引用
pub async fn get_client_mut(client: &SharedRedisClient) -> tokio::sync::RwLockWriteGuard<'_, RedisClient> {
    client.write().await
}

// ==================== 全局单例 ====================

static GLOBAL_REDIS_CLIENT: OnceLock<RedisClient> = OnceLock::new();
static GLOBAL_REDIS_CLIENT_MUTEX: OnceLock<Mutex<RedisClient>> = OnceLock::new();

/// 全局 Redis 客户端管理器
pub struct GlobalRedisClient;

impl GlobalRedisClient {
    /// 初始化全局 Redis 客户端
    pub fn initialize(config: RedisConfig) -> Result<()> {
        let runtime = tokio::runtime::Handle::current();
        let client = runtime.block_on(async {
            RedisClient::new(config).await
        })?;
        
        GLOBAL_REDIS_CLIENT
            .set(client.clone())
            .map_err(|_| Error::ConfigError("全局 Redis 客户端已初始化".to_string()))?;
        
        GLOBAL_REDIS_CLIENT_MUTEX
            .set(Mutex::new(client))
            .map_err(|_| Error::ConfigError("全局 Redis 客户端 Mutex 已初始化".to_string()))
    }

    /// 初始化全局 Redis 客户端（带缓存和锁配置）
    pub fn initialize_with_configs(
        redis_config: RedisConfig,
        cache_config: CacheConfig,
        lock_config: LockConfig,
    ) -> Result<()> {
        let runtime = tokio::runtime::Handle::current();
        let client = runtime.block_on(async {
            RedisClient::new_with_configs(redis_config, cache_config, lock_config).await
        })?;
        
        GLOBAL_REDIS_CLIENT
            .set(client.clone())
            .map_err(|_| Error::ConfigError("全局 Redis 客户端已初始化".to_string()))?;
        
        GLOBAL_REDIS_CLIENT_MUTEX
            .set(Mutex::new(client))
            .map_err(|_| Error::ConfigError("全局 Redis 客户端 Mutex 已初始化".to_string()))
    }

    /// 获取全局 Redis 客户端引用
    pub fn get() -> &'static RedisClient {
        GLOBAL_REDIS_CLIENT.get().expect(
            "Redis 客户端未初始化，请先调用 GlobalRedisClient::initialize() 或 GlobalRedisClient::initialize_with_configs()"
        )
    }

    /// 获取全局 Redis 客户端可变引用
    pub fn get_mut() -> std::sync::MutexGuard<'static, RedisClient> {
        GLOBAL_REDIS_CLIENT_MUTEX.get().expect(
            "Redis 客户端未初始化，请先调用 GlobalRedisClient::initialize() 或 GlobalRedisClient::initialize_with_configs()"
        ).lock().unwrap()
    }

    /// 检查是否已初始化
    pub fn is_initialized() -> bool {
        GLOBAL_REDIS_CLIENT.get().is_some()
    }

    /// 获取全局 Redis 客户端克隆
    pub fn get_cloned() -> RedisClient {
        Self::get().clone()
    }
}
