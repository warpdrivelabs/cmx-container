//! Redis 缓存模块
//!
//! 提供 Redis 缓存客户端和分布式锁的初始化功能。

use cmx_buffer::{GlobalCacheManager, GlobalLockManager, RedisClient, RedisConfig};
use cmx_utils::ConfigManager;
use tracing::info;

pub use crate::Error;

/// 初始化缓存。
///
/// 创建唯一的 RedisClient，由 GlobalCacheManager 和 GlobalLockManager 共享，
/// 避免创建多个独立的 bb8 连接池。
///
/// # Returns
///
/// * `Ok(())` - 缓存初始化成功
/// * `Err(Error::ServerSetup)` - Redis 配置或连接失败
pub async fn init_cache() -> crate::Result<()> {
    let config = ConfigManager::global();
    let _url_value = config.get("redis.url")
        .ok_or_else(|| Error::ServerSetup("无法从配置管理器获取 redis.url".into()))?;

    let redis_config = config.get_as::<RedisConfig>("redis")
        .map_err(|e| Error::ServerSetup(format!("Redis 配置解析失败: {}", e)))?;

    let client = RedisClient::new(redis_config)
        .await
        .map_err(|e| Error::ServerSetup(format!("Redis 客户端创建失败: {}", e)))?;

    GlobalCacheManager::initialize_with_client(client.clone())
        .map_err(|e| Error::ServerSetup(format!("Redis 缓存初始化失败: {}", e)))?;
    info!("redis缓存初始化完成");

    GlobalLockManager::initialize_with_client(client)
        .map_err(|e| Error::ServerSetup(format!("Redis 分布式锁初始化失败: {}", e)))?;
    info!("redis分布式锁初始化完成");

    Ok(())
}
