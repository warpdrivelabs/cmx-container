//! 缓存操作模块入口
//!
//! 提供缓存管理器 `CacheManager`，用于统一管理各种缓存操作。

pub mod ops;
pub mod pubsub;
pub mod set;
pub mod sorted_set;
pub mod ttl;

pub use ops::CacheOps;
pub use pubsub::{
    ChannelHandler, FnChannelHandler, GlobalSubscriber, GlobalSubscriberManager,
    PubSubOps,
};
pub use set::SetOps;
pub use sorted_set::SortedSetOps;
pub use ttl::TtlOps;

use crate::client::RedisClient;
use crate::config::{CacheConfig, LockConfig, RedisConfig};
use crate::error::{Error, Result};
use std::sync::Arc;
use std::sync::OnceLock;

/// 缓存管理器 - 提供统一的缓存操作入口
#[derive(Clone)]
pub struct CacheManager {
    client: RedisClient,
}

impl CacheManager {
    /// 创建新的缓存管理器
    pub fn new(client: RedisClient) -> Self {
        Self { client }
    }

    /// 获取字符串缓存操作器
    pub fn ops(&self) -> CacheOps {
        CacheOps::new(self.client.clone())
    }

    /// 获取 TTL 操作器
    pub fn ttl(&self) -> TtlOps {
        TtlOps::new(self.client.clone())
    }

    /// 获取有序集合操作器
    pub fn sorted_set(&self) -> SortedSetOps {
        SortedSetOps::new(self.client.clone())
    }

    /// 获取集合操作器
    pub fn set(&self) -> SetOps {
        SetOps::new(self.client.clone())
    }

    /// 获取发布/订阅操作器
    pub fn pubsub(&self) -> PubSubOps {
        PubSubOps::new(self.client.clone())
    }

    /// 获取内部客户端引用
    pub fn client(&self) -> &RedisClient {
        &self.client
    }
}

static GLOBAL_CACHE_MANAGER: OnceLock<Arc<CacheManager>> = OnceLock::new();

/// 全局缓存管理器 - 提供应用级别的单例访问
pub struct GlobalCacheManager;

impl GlobalCacheManager {
    /// 初始化全局缓存管理器
    pub async fn initialize(redis_config: RedisConfig) -> Result<()> {
        let client = RedisClient::new(redis_config).await?;
        let cache_manager = CacheManager::new(client);

        GLOBAL_CACHE_MANAGER
            .set(Arc::new(cache_manager))
            .map_err(|_| Error::ConfigError("全局缓存管理器已初始化".to_string()))?;
        Ok(())
    }

    /// 初始化全局缓存管理器（带配置）
    pub async fn initialize_with_configs(
        redis_config: RedisConfig,
        cache_config: CacheConfig,
        lock_config: LockConfig,
    ) -> Result<()> {
        let client = RedisClient::new_with_configs(redis_config, cache_config, lock_config).await?;
        let cache_manager = CacheManager::new(client);

        GLOBAL_CACHE_MANAGER
            .set(Arc::new(cache_manager))
            .map_err(|_| Error::ConfigError("全局缓存管理器已初始化".to_string()))?;
        Ok(())
    }

    /// 获取全局缓存管理器引用
    pub fn get() -> &'static Arc<CacheManager> {
        GLOBAL_CACHE_MANAGER.get().expect(
            "缓存管理器未初始化，请先调用 GlobalCacheManager::initialize() 或 GlobalCacheManager::initialize_with_configs()"
        )
    }

    /// 使用已有的 RedisClient 初始化全局缓存管理器
    pub fn initialize_with_client(client: RedisClient) -> Result<()> {
        let cache_manager = CacheManager::new(client);
        GLOBAL_CACHE_MANAGER
            .set(Arc::new(cache_manager))
            .map_err(|_| Error::ConfigError("全局缓存管理器已初始化".to_string()))?;
        Ok(())
    }

    /// 检查是否已初始化
    pub fn is_initialized() -> bool {
        GLOBAL_CACHE_MANAGER.get().is_some()
    }
}
