//! 全局配置中心存储器
//!
//! 提供全局访问 ConfigCenter 的能力。
//! 在应用启动时通过 `set` 方法设置，之后可通过 `get` 方法获取。

use std::sync::{Arc, OnceLock};

use crate::config_center::trait_rs::ConfigCenter;

/// 全局配置中心错误类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GlobalConfigCenterError(&'static str);

impl GlobalConfigCenterError {
    pub const ALREADY_SET: Self = GlobalConfigCenterError("配置中心已初始化，无法重复设置");
}

/// 全局配置中心存储器
///
/// 在应用启动时通过 `set` 方法设置，之后可通过 `get` 方法获取。
pub struct GlobalConfigCenter;

static CONFIG_CENTER: OnceLock<Arc<dyn ConfigCenter>> = OnceLock::new();

impl GlobalConfigCenter {
    /// 设置全局配置中心实例
    pub fn set(config_center: Arc<dyn ConfigCenter>) -> Result<(), GlobalConfigCenterError> {
        CONFIG_CENTER
            .set(config_center)
            .map_err(|_| GlobalConfigCenterError::ALREADY_SET)
    }

    /// 获取全局配置中心实例
    ///
    /// # Panics
    /// 如果未初始化则 panic
    pub fn get() -> &'static Arc<dyn ConfigCenter> {
        CONFIG_CENTER
            .get()
            .expect("GlobalConfigCenter 未初始化，请先调用 GlobalConfigCenter::set()")
    }

    /// 检查是否已初始化
    pub fn is_initialized() -> bool {
        CONFIG_CENTER.get().is_some()
    }
}
