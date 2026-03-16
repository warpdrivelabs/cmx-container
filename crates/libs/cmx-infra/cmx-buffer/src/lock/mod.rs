pub mod manager;

pub use manager::{LockGuard, LockManager};

use crate::client::RedisClient;
use crate::config::{CacheConfig, LockConfig, RedisConfig};
use crate::error::{Error, Result};
use std::sync::OnceLock;
use std::sync::Mutex;

///! 分布式锁模块入口

/// 作者: AI Assistant
/// 日期: 2026-03-16

/// 创建分布式锁管理器
pub fn create_lock_manager(client: RedisClient) -> LockManager {
    LockManager::new_with_default_config(client)
}

/// 创建分布式锁管理器（带自定义配置）
pub fn create_lock_manager_with_config(client: RedisClient, config: LockConfig) -> LockManager {
    LockManager::new(client, config)
}

// ==================== 全局单例 ====================

static GLOBAL_LOCK_MANAGER: OnceLock<LockManager> = OnceLock::new();
static GLOBAL_LOCK_MANAGER_MUTEX: OnceLock<Mutex<LockManager>> = OnceLock::new();

/// 全局分布式锁管理器
pub struct GlobalLockManager;

impl GlobalLockManager {
    /// 初始化全局锁管理器（使用默认配置）
    pub fn initialize(redis_config: RedisConfig) -> Result<()> {
        let runtime = tokio::runtime::Handle::current();
        let client = runtime.block_on(async {
            RedisClient::new(redis_config).await
        })?;
        
        let lock_manager = LockManager::new_with_default_config(client);
        
        GLOBAL_LOCK_MANAGER
            .set(lock_manager.clone())
            .map_err(|_| Error::ConfigError("全局锁管理器已初始化".to_string()))?;
        
        GLOBAL_LOCK_MANAGER_MUTEX
            .set(Mutex::new(lock_manager))
            .map_err(|_| Error::ConfigError("全局锁管理器 Mutex 已初始化".to_string()))
    }

    /// 初始化全局锁管理器（带 Redis 配置）
    pub fn initialize_with_redis_config(redis_config: RedisConfig, lock_config: LockConfig) -> Result<()> {
        let runtime = tokio::runtime::Handle::current();
        let client = runtime.block_on(async {
            RedisClient::new(redis_config).await
        })?;
        
        let lock_manager = LockManager::new(client, lock_config);
        
        GLOBAL_LOCK_MANAGER
            .set(lock_manager.clone())
            .map_err(|_| Error::ConfigError("全局锁管理器已初始化".to_string()))?;
        
        GLOBAL_LOCK_MANAGER_MUTEX
            .set(Mutex::new(lock_manager))
            .map_err(|_| Error::ConfigError("全局锁管理器 Mutex 已初始化".to_string()))
    }

    /// 初始化全局锁管理器（带完整配置）
    pub fn initialize_with_configs(
        redis_config: RedisConfig,
        cache_config: CacheConfig,
        lock_config: LockConfig,
    ) -> Result<()> {
        let runtime = tokio::runtime::Handle::current();
        let client = runtime.block_on(async {
            RedisClient::new_with_configs(redis_config, cache_config.clone(), lock_config.clone()).await
        })?;
        
        let lock_manager = LockManager::new(client, lock_config);
        
        GLOBAL_LOCK_MANAGER
            .set(lock_manager.clone())
            .map_err(|_| Error::ConfigError("全局锁管理器已初始化".to_string()))?;
        
        GLOBAL_LOCK_MANAGER_MUTEX
            .set(Mutex::new(lock_manager))
            .map_err(|_| Error::ConfigError("全局锁管理器 Mutex 已初始化".to_string()))
    }

    /// 获取全局锁管理器引用
    pub fn get() -> &'static LockManager {
        GLOBAL_LOCK_MANAGER.get().expect(
            "锁管理器未初始化，请先调用 GlobalLockManager::initialize() 或 GlobalLockManager::initialize_with_configs()"
        )
    }

    /// 获取全局锁管理器可变引用
    pub fn get_mut() -> std::sync::MutexGuard<'static, LockManager> {
        GLOBAL_LOCK_MANAGER_MUTEX.get().expect(
            "锁管理器未初始化，请先调用 GlobalLockManager::initialize() 或 GlobalLockManager::initialize_with_configs()"
        ).lock().unwrap()
    }

    /// 检查是否已初始化
    pub fn is_initialized() -> bool {
        GLOBAL_LOCK_MANAGER.get().is_some()
    }

    /// 获取全局锁管理器克隆
    pub fn get_cloned() -> LockManager {
        Self::get().clone()
    }
}
