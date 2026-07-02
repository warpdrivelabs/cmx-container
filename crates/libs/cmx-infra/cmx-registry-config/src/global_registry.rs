//! 全局服务注册中心存储器。
//!
//! 提供应用层访问 [`ServiceRegistry`] 的全局单例。
//! 在应用启动时通过 [`GlobalServiceRegistry::set`] 设置一次，
//! 之后任意位置可通过 [`GlobalServiceRegistry::get`] 获取访问。
//!
//! # 线程安全
//!
//! 内部使用 `OnceLock<Arc<dyn ServiceRegistry>>`，保证：
//! - 初始化操作线程安全。
//! - 多个调用方并发读取无需加锁。
//! - 整个进程生命周期内只能成功设置一次。

use std::sync::{Arc, OnceLock};

use crate::error::GlobalStorageError;
use crate::registry::ServiceRegistry;

/// 全局服务注册中心存储器。
///
/// 通过关联函数（`set` / `get` / `is_initialized`）操作 `OnceLock` 单例。
/// 该类型本身无字段，所有状态存储在模块级静态变量中。
pub struct GlobalServiceRegistry;

/// 注册中心单例存储。`OnceLock` 保证线程安全的延迟初始化与一次性写入。
static REGISTRY: OnceLock<Arc<dyn ServiceRegistry>> = OnceLock::new();

impl GlobalServiceRegistry {
    /// 设置全局服务注册中心实例。
    ///
    /// 整个进程生命周期内只能成功调用一次。
    ///
    /// # Arguments
    ///
    /// * `registry` - 注册中心的 `Arc` 动态分发实例。
    ///
    /// # Returns
    ///
    /// * `Ok(())` - 首次设置成功。
    /// * `Err(GlobalStorageError::ALREADY_SET)` - 已被设置过。
    pub fn set(registry: Arc<dyn ServiceRegistry>) -> Result<(), GlobalStorageError> {
        REGISTRY
            .set(registry)
            .map_err(|_| GlobalStorageError::ALREADY_SET)
    }

    /// 获取全局服务注册中心实例。
    ///
    /// # Panics
    ///
    /// 如果未调用 [`Self::set`] 完成初始化则 panic。
    ///
    /// # Returns
    ///
    /// 返回 `&'static Arc<dyn ServiceRegistry>`，可直接用于 `dyn ServiceRegistry` 调用。
    pub fn get() -> &'static Arc<dyn ServiceRegistry> {
        REGISTRY
            .get()
            .expect("GlobalServiceRegistry 未初始化，请先调用 GlobalServiceRegistry::set()")
    }

    /// 检查是否已初始化。
    ///
    /// # Returns
    ///
    /// * `true` - 已调用过 [`Self::set`]。
    /// * `false` - 尚未设置。
    pub fn is_initialized() -> bool {
        REGISTRY.get().is_some()
    }
}
