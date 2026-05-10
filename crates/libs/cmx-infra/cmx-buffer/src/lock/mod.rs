pub mod manager;

pub use manager::{LockGuard, LockManager};

use crate::client::RedisClient;
use crate::config::{CacheConfig, LockConfig, RedisConfig};
use crate::error::{Error, Result};
use std::sync::{Arc, OnceLock};

/// 创建分布式锁管理器
pub fn create_lock_manager(client: RedisClient) -> LockManager {
    LockManager::new_with_default_config(client)
}

/// 创建分布式锁管理器（带自定义配置）
pub fn create_lock_manager_with_config(client: RedisClient, config: LockConfig) -> LockManager {
    LockManager::new(client, config)
}

static GLOBAL_LOCK_MANAGER: OnceLock<Arc<LockManager>> = OnceLock::new();

/// 全局分布式锁管理器
pub struct GlobalLockManager;

impl GlobalLockManager {
    /// 初始化全局锁管理器（使用默认配置）
    pub async fn initialize(redis_config: RedisConfig) -> Result<()> {
        let client = RedisClient::new(redis_config).await?;
        let lock_manager = LockManager::new_with_default_config(client);

        GLOBAL_LOCK_MANAGER
            .set(Arc::new(lock_manager))
            .map_err(|_| Error::ConfigError("全局锁管理器已初始化".to_string()))?;

        Ok(())
    }

    /// 初始化全局锁管理器（带 Redis 配置）
    pub async fn initialize_with_redis_config(
        redis_config: RedisConfig,
        lock_config: LockConfig,
    ) -> Result<()> {
        let client = RedisClient::new(redis_config).await?;
        let lock_manager = LockManager::new(client, lock_config);
        GLOBAL_LOCK_MANAGER
            .set(Arc::new(lock_manager))
            .map_err(|_| Error::ConfigError("全局锁管理器已初始化".to_string()))?;

        Ok(())
    }

    /// 初始化全局锁管理器（带完整配置）
    pub async fn initialize_with_configs(
        redis_config: RedisConfig,
        cache_config: CacheConfig,
        lock_config: LockConfig,
    ) -> Result<()> {
        let client =
            RedisClient::new_with_configs(redis_config, cache_config.clone(), lock_config.clone())
                .await?;

        let lock_manager = LockManager::new(client, lock_config);

        GLOBAL_LOCK_MANAGER
            .set(Arc::new(lock_manager))
            .map_err(|_| Error::ConfigError("全局锁管理器已初始化".to_string()))?;

        Ok(())
    }

    /// 获取全局锁管理器引用
    pub fn get() -> &'static Arc<LockManager> {
        GLOBAL_LOCK_MANAGER.get().expect(
            "锁管理器未初始化，请先调用 GlobalLockManager::initialize() 或 GlobalLockManager::initialize_with_configs()"
        )
    }

    /// 检查是否已初始化
    pub fn is_initialized() -> bool {
        GLOBAL_LOCK_MANAGER.get().is_some()
    }

    /// 使用已有的 RedisClient 初始化全局锁管理器
    pub fn initialize_with_client(client: RedisClient) -> Result<()> {
        let lock_manager = LockManager::new_with_default_config(client);
        GLOBAL_LOCK_MANAGER
            .set(Arc::new(lock_manager))
            .map_err(|_| Error::ConfigError("全局锁管理器已初始化".to_string()))?;
        Ok(())
    }
}
