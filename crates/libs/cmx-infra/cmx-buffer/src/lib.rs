/**
 * @Author: AI Assistant
 * @Date: 2026-03-16
 * @Describe: cmx-buffer 模块入口
 */

pub mod cache;
pub mod client;
pub mod config;
pub mod error;
pub mod lock;
pub mod logging;

pub use cache::{
    CacheManager, CacheOps, GlobalCacheManager, 
    SortedSetOps, SetOps, PubSubOps,
    PubSubMessage, Subscriber, SharedSubscriber,
    TtlOps
};
pub use client::{
    create_shared_client, get_client, get_client_mut, GlobalRedisClient, RedisClient, SharedRedisClient
};
pub use config::{CacheConfig, LockConfig, RedisConfig};
pub use error::{Error, Result};
pub use lock::{create_lock_manager, create_lock_manager_with_config, GlobalLockManager, LockGuard, LockManager};

pub type BufferResult<T> = Result<T>;

pub async fn create_redis_client(url: &str) -> Result<RedisClient> {
    let config = RedisConfig::new(url);
    RedisClient::new(config).await
}

pub async fn create_redis_client_with_config(config: RedisConfig) -> Result<RedisClient> {
    RedisClient::new(config).await
}
