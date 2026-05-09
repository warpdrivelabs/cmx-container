//! Redis 客户端封装模块
//!
//! 提供 `RedisClient` 结构体，用于封装 bb8 连接池和 Redis 操作。

use crate::config::{CacheConfig, LockConfig, RedisConfig};
use crate::error::{Error, Result};
use crate::logging::ConnLog;
use bb8::Pool;
use bb8_redis::RedisConnectionManager;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

/// Redis 客户端包装器（使用 bb8 连接池）
///
/// 封装了 Redis 连接池和配置，提供缓存操作和锁管理功能。
#[derive(Clone)]
pub struct RedisClient {
    pool: Pool<RedisConnectionManager>,
    config: RedisConfig,
    cache_config: CacheConfig,
    lock_config: LockConfig,
}

impl RedisClient {
    /// 从配置创建新的 Redis 客户端（使用默认配置）
    ///
    /// # 参数
    /// * `config` - Redis 配置
    ///
    /// # 返回值
    /// * 成功返回 `RedisClient`
    /// * 失败返回 `Error`
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
            "创建 Redis 客户端（bb8 连接池）"
        );

        let manager = RedisConnectionManager::new(config.url.as_str())
            .map_err(|e| Error::ConnectionError(e.to_string()))?;

        let pool = Pool::builder()
            .max_size(config.pool_size as u32)
            .build(manager)
            .await
            .map_err(|e| Error::PoolError(e.to_string()))?;

        ConnLog::connected(&config.url);

        Ok(Self {
            pool,
            config,
            cache_config,
            lock_config,
        })
    }

    /// 获取连接池
    pub fn pool(&self) -> &Pool<RedisConnectionManager> {
        &self.pool
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
        if let Ok(mut conn) = self.pool.get().await {
            let result: std::result::Result<String, redis::RedisError> = redis::cmd("PING")
                .query_async(&mut *conn)
                .await;
            return result.is_ok();
        }
        false
    }

    /// 关闭连接池
    pub async fn close(&self) -> Result<()> {
        info!("关闭 Redis 连接池");
        drop(self.pool.clone());
        Ok(())
    }

    /// 获取连接（从连接池获取）
    pub async fn get_connection(&self) -> Result<bb8::PooledConnection<'_, RedisConnectionManager>> {
        self.pool.get().await.map_err(|e| Error::PoolError(e.to_string()))
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
