//! cmx-buffer 模块入口
//!
//! 提供 Redis 缓存客户端和分布式锁功能的封装。
//! 支持单机和集群两种 Redis 模式，缓存操作（字符串、集合、有序集合、发布/订阅），
//! 以及分布式锁的获取、释放和自动续期。

pub mod cache;
pub mod client;
pub mod config;
pub mod error;
pub mod host_functions;
pub mod lock;
pub mod logging;

pub use cache::{
    CacheManager, CacheOps, GlobalCacheManager,
    PubSubMessage, PubSubOps, SetOps, SharedSubscriber,
    SortedSetOps, Subscriber, SubscriberBuilder, TtlOps,
};
pub use client::{
    create_shared_client, get_client, get_client_mut, GlobalRedisClient, RedisClient,
    RedisConnectionRef, SharedRedisClient,
};
pub use config::{CacheConfig, LockConfig, RedisConfig, RedisMode};
pub use error::{Error, Result};
pub use lock::{
    create_lock_manager, create_lock_manager_with_config, GlobalLockManager, LockGuard, LockManager,
};

pub use host_functions::BufferHostFunctions;

/// cmx-buffer 模块的结果类型别名
pub type BufferResult<T> = Result<T>;

/// 根据 URL 创建 Redis 客户端（单机模式）
pub async fn create_redis_client(url: &str) -> Result<RedisClient> {
    let config = RedisConfig::new(url);
    RedisClient::new(config).await
}

/// 根据配置创建 Redis 客户端
pub async fn create_redis_client_with_config(config: RedisConfig) -> Result<RedisClient> {
    RedisClient::new(config).await
}
