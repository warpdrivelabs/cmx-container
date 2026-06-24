//! 全局服务实例缓存存储器。
//!
//! 提供应用层访问 [`ServiceInstanceCache`] 的全局单例。
//! 在应用启动时通过 [`GlobalServiceInstanceCache::set`] 设置一次，
//! 之后任意位置可通过 [`GlobalServiceInstanceCache::get`] 获取访问。

use std::sync::{Arc, OnceLock};

use crate::registry::ServiceInstanceCache;

/// 全局服务实例缓存错误类型。
#[derive(thiserror::Error, Debug, Clone, Copy, PartialEq, Eq)]
#[error("{0}")]
pub struct GlobalInstanceCacheError(&'static str);

impl GlobalInstanceCacheError {
    /// 表示缓存已被设置过，重复设置会触发该错误。
    pub const ALREADY_SET: Self = GlobalInstanceCacheError("GlobalServiceInstanceCache 已初始化，无法重复设置");
}

/// 全局服务实例缓存存储器。
///
/// 通过关联函数（`set` / `get` / `is_initialized`）操作 `OnceLock` 单例。
/// 该类型本身无字段，所有状态存储在模块级静态变量中。
pub struct GlobalServiceInstanceCache;

/// 缓存单例存储。`OnceLock` 保证线程安全的延迟初始化与一次性写入。
static CACHE: OnceLock<Arc<ServiceInstanceCache>> = OnceLock::new();

impl GlobalServiceInstanceCache {
    /// 设置全局缓存实例。
    ///
    /// 整个进程生命周期内只能成功调用一次。
    ///
    /// # Returns
    ///
    /// * `Ok(())` - 首次设置成功。
    /// * `Err(GlobalInstanceCacheError::ALREADY_SET)` - 已被设置过。
    pub fn set(cache: Arc<ServiceInstanceCache>) -> Result<(), GlobalInstanceCacheError> {
        CACHE
            .set(cache)
            .map_err(|_| {
                tracing::warn!("GlobalServiceInstanceCache 重复初始化被拒绝");
                GlobalInstanceCacheError::ALREADY_SET
            })?;
        tracing::info!("GlobalServiceInstanceCache 初始化完成");
        Ok(())
    }

    /// 获取全局缓存实例。
    ///
    /// # Panics
    ///
    /// 如果未调用 [`Self::set`] 完成初始化则 panic。
    pub fn get() -> &'static Arc<ServiceInstanceCache> {
        CACHE
            .get()
            .expect("GlobalServiceInstanceCache 未初始化，请先调用 GlobalServiceInstanceCache::set()")
    }

    /// 检查是否已初始化。
    pub fn is_initialized() -> bool {
        CACHE.get().is_some()
    }
}
