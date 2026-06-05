//! 全局注册中心存储器
//!
//! 提供全局访问 ServiceRegistry 的能力。
//! 在应用启动时通过 `set` 方法设置，之后可通过 `get` 方法获取。

use std::sync::{Arc, OnceLock};

use crate::registry::trait_rs::ServiceRegistry;

/// 全局注册中心错误类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GlobalRegistryError(&'static str);

impl GlobalRegistryError {
    pub const ALREADY_SET: Self = GlobalRegistryError("注册中心已初始化，无法重复设置");
}

/// 全局注册中心存储器
///
/// 在应用启动时通过 `set` 方法设置，之后可通过 `get` 方法获取。
pub struct GlobalRegistry;

static REGISTRY: OnceLock<Arc<dyn ServiceRegistry>> = OnceLock::new();

impl GlobalRegistry {
    /// 设置全局注册中心实例
    pub fn set(registry: Arc<dyn ServiceRegistry>) -> Result<(), GlobalRegistryError> {
        REGISTRY.set(registry).map_err(|_| GlobalRegistryError::ALREADY_SET)
    }

    /// 获取全局注册中心实例
    ///
    /// # Panics
    /// 如果未初始化则 panic
    pub fn get() -> &'static Arc<dyn ServiceRegistry> {
        REGISTRY
            .get()
            .expect("GlobalRegistry 未初始化，请先调用 GlobalRegistry::set()")
    }

    /// 检查是否已初始化
    pub fn is_initialized() -> bool {
        REGISTRY.get().is_some()
    }
}
