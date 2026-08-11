//! Redis 缓存模块
//!
//! 提供 Redis 缓存客户端和分布式锁的初始化功能。
//! client-init 逻辑已下沉到 cmx-service-base；本模块保留「从 ConfigManager 读 RedisConfig」再委托。

use cmx_buffer::RedisConfig;
use cmx_utils::ConfigManager;

pub use crate::Error;

/// 初始化缓存。
///
/// 从 ConfigManager 读 `redis` 配置，委托 `cmx_service_base::init_cache` 创建唯一 RedisClient，
/// 由 GlobalCacheManager 和 GlobalLockManager 共享（避免多个独立 bb8 连接池）。
///
/// # Returns
///
/// * `Ok(())` - 缓存初始化成功
/// * `Err(Error::ServerSetup)` - Redis 配置或连接失败
pub async fn init_cache() -> crate::Result<()> {
    let config = ConfigManager::global();
    let _url_value = config
        .get("redis.url")
        .ok_or_else(|| Error::ServerSetup("无法从配置管理器获取 redis.url".into()))?;

    let redis_config = config
        .get_as::<RedisConfig>("redis")
        .map_err(|e| Error::ServerSetup(format!("Redis 配置解析失败: {}", e)))?;

    cmx_service_base::init_cache(redis_config)
        .await
        .map_err(|e| Error::ServerSetup(format!("Redis 初始化失败: {}", e)))?;

    Ok(())
}
