//! 全局配置中心存储器。
//!
//! 提供应用层访问 [`ConfigCenter`] 的全局单例。
//! 在应用启动时通过 [`GlobalConfigCenter::set`] 设置一次，
//! 之后任意位置可通过 [`GlobalConfigCenter::get`] 获取访问。
//!
//! # 线程安全
//!
//! 内部使用 `OnceLock<Arc<dyn ConfigCenter>>`，保证：
//! - 初始化操作线程安全。
//! - 多个调用方并发读取无需加锁。
//! - 整个进程生命周期内只能成功设置一次。

use std::sync::{Arc, OnceLock};

use crate::config_center::trait_rs::ConfigCenter;

/// 全局配置中心错误类型。
///
/// 用于 `set` 操作的失败情形（如重复初始化），包含人类可读的错误描述。
#[derive(thiserror::Error, Debug, Clone, Copy, PartialEq, Eq)]
#[error("{0}")]
pub struct GlobalConfigCenterError(&'static str);

impl GlobalConfigCenterError {
    /// 表示配置中心已被设置过，重复设置会触发该错误。
    pub const ALREADY_SET: Self = GlobalConfigCenterError("配置中心已初始化，无法重复设置");
}

/// 全局配置中心存储器。
///
/// 通过关联函数（`set` / `get` / `is_initialized`）操作 `OnceLock` 单例。
/// 该类型本身无字段，所有状态存储在模块级静态变量中。
pub struct GlobalConfigCenter;

/// 配置中心单例存储。`OnceLock` 保证线程安全的延迟初始化与一次性写入。
static CONFIG_CENTER: OnceLock<Arc<dyn ConfigCenter>> = OnceLock::new();

impl GlobalConfigCenter {
    /// 设置全局配置中心实例。
    ///
    /// 整个进程生命周期内只能成功调用一次。
    ///
    /// # Arguments
    ///
    /// * `config_center` - 配置中心的 `Arc` 动态分发实例。
    ///
    /// # Returns
    ///
    /// * `Ok(())` - 首次设置成功。
    /// * `Err(GlobalConfigCenterError::ALREADY_SET)` - 已被设置过。
    pub fn set(config_center: Arc<dyn ConfigCenter>) -> Result<(), GlobalConfigCenterError> {
        CONFIG_CENTER
            .set(config_center)
            .map_err(|_| GlobalConfigCenterError::ALREADY_SET)
    }

    /// 获取全局配置中心实例。
    ///
    /// # Panics
    ///
    /// 如果未调用 [`Self::set`] 完成初始化则 panic。
    ///
    /// # Returns
    ///
    /// 返回 `&'static Arc<dyn ConfigCenter>`，可直接用于 `dyn ConfigCenter` 调用。
    pub fn get() -> &'static Arc<dyn ConfigCenter> {
        CONFIG_CENTER
            .get()
            .expect("GlobalConfigCenter 未初始化，请先调用 GlobalConfigCenter::set()")
    }

    /// 检查是否已初始化。
    ///
    /// # Returns
    ///
    /// * `true` - 已调用过 [`Self::set`]。
    /// * `false` - 尚未设置。
    pub fn is_initialized() -> bool {
        CONFIG_CENTER.get().is_some()
    }
}
