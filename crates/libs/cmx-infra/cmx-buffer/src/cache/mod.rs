//! 缓存操作模块入口
//!
//! 提供缓存管理器 `CacheManager`，用于统一管理各种缓存操作。

pub mod ops;
pub mod pubsub;
pub mod set;
pub mod sorted_set;
pub mod ttl;

pub use ops::CacheOps;
pub use pubsub::{PubSubMessage, PubSubOps, SharedSubscriber, Subscriber};
pub use set::SetOps;
pub use sorted_set::SortedSetOps;
pub use ttl::TtlOps;

use crate::client::RedisClient;
use crate::config::{CacheConfig, LockConfig, RedisConfig};
use crate::error::{Error, Result};
use std::sync::Mutex;
use std::sync::OnceLock;

/// 缓存管理器 - 提供统一的缓存操作入口
#[derive(Clone)]
pub struct CacheManager {
    client: RedisClient,
}

impl CacheManager {
    /// 创建新的缓存管理器
    ///
    /// # 参数
    /// * `client` - Redis 客户端实例
    ///
    /// # 返回值
    /// * 缓存管理器实例
    pub fn new(client: RedisClient) -> Self {
        Self { client }
    }

    /// 获取字符串缓存操作器
    ///
    /// # 返回值
    /// * CacheOps 实例，用于基本的字符串缓存操作
    pub fn ops(&self) -> CacheOps {
        CacheOps::new(self.client.clone())
    }

    /// 获取 TTL 操作器
    ///
    /// # 返回值
    /// * TtlOps 实例，用于管理键的过期时间
    pub fn ttl(&self) -> TtlOps {
        TtlOps::new(self.client.clone())
    }

    /// 获取有序集合操作器
    ///
    /// # 返回值
    /// * SortedSetOps 实例，用于有序集合操作
    pub fn sorted_set(&self) -> SortedSetOps {
        SortedSetOps::new(self.client.clone())
    }

    /// 获取集合操作器
    ///
    /// # 返回值
    /// * SetOps 实例，用于集合操作
    pub fn set(&self) -> SetOps {
        SetOps::new(self.client.clone())
    }

    /// 获取发布/订阅操作器
    ///
    /// # 返回值
    /// * PubSubOps 实例，用于发布/订阅操作
    pub fn pubsub(&self) -> PubSubOps {
        PubSubOps::new(self.client.clone())
    }

    /// 获取内部客户端引用
    ///
    /// # 返回值
    /// * Redis 客户端引用
    pub fn client(&self) -> &RedisClient {
        &self.client
    }
}

// ==================== 全局单例 ====================

static GLOBAL_CACHE_MANAGER: OnceLock<CacheManager> = OnceLock::new();
static GLOBAL_CACHE_MANAGER_MUTEX: OnceLock<Mutex<CacheManager>> = OnceLock::new();

/// 全局缓存管理器 - 提供应用级别的单例访问
pub struct GlobalCacheManager;

impl GlobalCacheManager {
    /// 初始化全局缓存管理器
    ///
    /// # 参数
    /// * `redis_config` - Redis 配置
    ///
    /// # 返回值
    /// * 初始化结果
    pub async fn initialize(redis_config: RedisConfig) -> Result<()> {
        let client = RedisClient::new(redis_config).await?;
        let cache_manager = CacheManager::new(client);

        GLOBAL_CACHE_MANAGER
            .set(cache_manager.clone())
            .map_err(|_| Error::ConfigError("全局缓存管理器已初始化".to_string()))?;

        GLOBAL_CACHE_MANAGER_MUTEX
            .set(Mutex::new(cache_manager))
            .map_err(|_| Error::ConfigError("全局缓存管理器 Mutex 已初始化".to_string()))
    }

    /// 初始化全局缓存管理器（带配置）
    ///
    /// # 参数
    /// * `redis_config` - Redis 配置
    /// * `cache_config` - 缓存配置
    /// * `lock_config` - 锁配置
    ///
    /// # 返回值
    /// * 初始化结果
    pub async fn initialize_with_configs(
        redis_config: RedisConfig,
        cache_config: CacheConfig,
        lock_config: LockConfig,
    ) -> Result<()> {
        let client = RedisClient::new_with_configs(redis_config, cache_config, lock_config).await?;
        let cache_manager = CacheManager::new(client);

        GLOBAL_CACHE_MANAGER
            .set(cache_manager.clone())
            .map_err(|_| Error::ConfigError("全局缓存管理器已初始化".to_string()))?;

        GLOBAL_CACHE_MANAGER_MUTEX
            .set(Mutex::new(cache_manager))
            .map_err(|_| Error::ConfigError("全局缓存管理器 Mutex 已初始化".to_string()))
    }

    /// 获取全局缓存管理器引用
    ///
    /// # 返回值
    /// * 缓存管理器引用
    ///
    /// # Panics
    /// 如果未初始化则 panic
    pub fn get() -> &'static CacheManager {
        GLOBAL_CACHE_MANAGER.get().expect(
            "缓存管理器未初始化，请先调用 GlobalCacheManager::initialize() 或 GlobalCacheManager::initialize_with_configs()"
        )
    }

    /// 获取全局缓存管理器可变引用
    ///
    /// # 返回值
    /// * 缓存管理器可变引用
    ///
    /// # Panics
    /// 如果未初始化则 panic
    pub fn get_mut() -> std::sync::MutexGuard<'static, CacheManager> {
        GLOBAL_CACHE_MANAGER_MUTEX.get().expect(
            "缓存管理器未初始化，请先调用 GlobalCacheManager::initialize() 或 GlobalCacheManager::initialize_with_configs()"
        ).lock().unwrap()
    }

    /// 检查是否已初始化
    ///
    /// # 返回值
    /// * 是否已初始化
    pub fn is_initialized() -> bool {
        GLOBAL_CACHE_MANAGER.get().is_some()
    }

    /// 获取全局缓存管理器克隆
    ///
    /// # 返回值
    /// * 缓存管理器克隆
    pub fn get_cloned() -> CacheManager {
        Self::get().clone()
    }
}
