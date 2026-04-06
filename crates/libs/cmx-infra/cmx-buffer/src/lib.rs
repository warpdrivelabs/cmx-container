//! cmx-buffer 模块入口
//!
//! 提供 Redis 缓存客户端和分布式锁功能的封装。
//! 支持缓存操作（字符串、集合、有序集合、发布/订阅），
//! 以及分布式锁的获取、释放和自动续期。
//!
//! # 模块结构
//! - `cache`: 缓存操作模块
//! - `client`: Redis 客户端封装
//! - `config`: 配置结构体定义
//! - `error`: 错误类型定义
//! - `lock`: 分布式锁模块
//! - `logging`: 日志辅助工具
//!
//! # 使用示例
//! ```ignore
//! use cmx_buffer::{create_redis_client, CacheManager};
//!
//! let client = create_redis_client("redis://127.0.0.1:6379").await?;
//! let cache = CacheManager::new(client);
//! cache.ops().set("key", "value").await?;
//! ```

pub mod cache;
pub mod client;
pub mod config;
pub mod error;
pub mod host_functions;
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

/// cmx-buffer 模块的结果类型别名
pub type BufferResult<T> = Result<T>;

/// 根据 URL 创建 Redis 客户端
///
/// # 参数
/// * `url` - Redis 连接地址
///
/// # 返回值
/// * 成功返回 `RedisClient`
/// * 失败返回 `Error`
pub async fn create_redis_client(url: &str) -> Result<RedisClient> {
    let config = RedisConfig::new(url);
    RedisClient::new(config).await
}

/// 根据配置创建 Redis 客户端
///
/// # 参数
/// * `config` - Redis 配置
///
/// # 返回值
/// * 成功返回 `RedisClient`
/// * 失败返回 `Error`
pub async fn create_redis_client_with_config(config: RedisConfig) -> Result<RedisClient> {
    RedisClient::new(config).await
}
