//! Redis 缓存 + 分布式锁初始化（feature `redis`）。
//!
//! 从 web-server 的 `config/cache.rs` 提取的纯 client-init 逻辑：创建唯一 `RedisClient`，
//! 由 `GlobalCacheManager` 与 `GlobalLockManager` 共享（避免多个独立连接池）。与原实现的区别：
//! **不读 ConfigManager**——接受传入的 `RedisConfig`，故本函数只依赖 cmx-buffer，让 `redis`
//! feature 保持轻。web-server 侧保留「读 ConfigManager 得 RedisConfig」再委托本函数。

use cmx_buffer::{GlobalCacheManager, GlobalLockManager, RedisClient, RedisConfig};
use tracing::info;

use crate::{BaseError, Result};

/// 用给定 `RedisConfig` 初始化全局缓存 + 分布式锁（共享同一 client）。
pub async fn init_cache(cfg: RedisConfig) -> Result<()> {
    let client = RedisClient::new(cfg)
        .await
        .map_err(|e| BaseError::Setup(format!("Redis 客户端创建失败: {e}")))?;

    GlobalCacheManager::initialize_with_client(client.clone())
        .map_err(|e| BaseError::Setup(format!("Redis 缓存初始化失败: {e}")))?;
    info!("redis缓存初始化完成");

    GlobalLockManager::initialize_with_client(client)
        .map_err(|e| BaseError::Setup(format!("Redis 分布式锁初始化失败: {e}")))?;
    info!("redis分布式锁初始化完成");

    Ok(())
}
