pub mod ops;
pub mod ttl;

pub use ops::CacheOps;
pub use ttl::TtlOps;

use crate::client::RedisClient;
use crate::config::{CacheConfig, LockConfig, RedisConfig};
use crate::error::{Error, Result};
use std::sync::OnceLock;
use std::sync::Mutex;

/**
 * @Author: AI Assistant
 * @Date: 2026-03-16
 * @Describe: 缓存操作模块入口
 */

/// 缓存管理器
#[derive(Clone)]
pub struct CacheManager {
    client: RedisClient,
}

impl CacheManager {
    /// 创建新的缓存管理器
    pub fn new(client: RedisClient) -> Self {
        Self { client }
    }

    /// 获取缓存操作器
    pub fn ops(&self) -> CacheOps {
        CacheOps::new(self.client.clone())
    }

    /// 获取 TTL 操作器
    pub fn ttl(&self) -> TtlOps {
        TtlOps::new(self.client.clone())
    }

    /// 获取内部客户端引用
    pub fn client(&self) -> &RedisClient {
        &self.client
    }
}

// ==================== 全局单例 ====================

static GLOBAL_CACHE_MANAGER: OnceLock<CacheManager> = OnceLock::new();
static GLOBAL_CACHE_MANAGER_MUTEX: OnceLock<Mutex<CacheManager>> = OnceLock::new();

/// 全局缓存管理器
pub struct GlobalCacheManager;

impl GlobalCacheManager {
    /// 初始化全局缓存管理器
    pub fn initialize(redis_config: RedisConfig) -> Result<()> {
        let runtime = tokio::runtime::Handle::current();
        let client = runtime.block_on(async {
            RedisClient::new(redis_config).await
        })?;
        
        let cache_manager = CacheManager::new(client);
        
        GLOBAL_CACHE_MANAGER
            .set(cache_manager.clone())
            .map_err(|_| Error::ConfigError("全局缓存管理器已初始化".to_string()))?;
        
        GLOBAL_CACHE_MANAGER_MUTEX
            .set(Mutex::new(cache_manager))
            .map_err(|_| Error::ConfigError("全局缓存管理器 Mutex 已初始化".to_string()))
    }

    /// 初始化全局缓存管理器（带配置）
    pub fn initialize_with_configs(
        redis_config: RedisConfig,
        cache_config: CacheConfig,
        lock_config: LockConfig,
    ) -> Result<()> {
        let runtime = tokio::runtime::Handle::current();
        let client = runtime.block_on(async {
            RedisClient::new_with_configs(redis_config, cache_config, lock_config).await
        })?;
        
        let cache_manager = CacheManager::new(client);
        
        GLOBAL_CACHE_MANAGER
            .set(cache_manager.clone())
            .map_err(|_| Error::ConfigError("全局缓存管理器已初始化".to_string()))?;
        
        GLOBAL_CACHE_MANAGER_MUTEX
            .set(Mutex::new(cache_manager))
            .map_err(|_| Error::ConfigError("全局缓存管理器 Mutex 已初始化".to_string()))
    }

    /// 获取全局缓存管理器引用
    pub fn get() -> &'static CacheManager {
        GLOBAL_CACHE_MANAGER.get().expect(
            "缓存管理器未初始化，请先调用 GlobalCacheManager::initialize() 或 GlobalCacheManager::initialize_with_configs()"
        )
    }

    /// 获取全局缓存管理器可变引用
    pub fn get_mut() -> std::sync::MutexGuard<'static, CacheManager> {
        GLOBAL_CACHE_MANAGER_MUTEX.get().expect(
            "缓存管理器未初始化，请先调用 GlobalCacheManager::initialize() 或 GlobalCacheManager::initialize_with_configs()"
        ).lock().unwrap()
    }

    /// 检查是否已初始化
    pub fn is_initialized() -> bool {
        GLOBAL_CACHE_MANAGER.get().is_some()
    }

    /// 获取全局缓存管理器克隆
    pub fn get_cloned() -> CacheManager {
        Self::get().clone()
    }
}
