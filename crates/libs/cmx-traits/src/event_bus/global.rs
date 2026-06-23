//! 全局事件总线单例管理。

use std::sync::OnceLock;

use super::bus::EventBus;
use crate::error::TraitError;

/// 全局事件总线实例。
static GLOBAL_EVENT_BUS: OnceLock<EventBus> = OnceLock::new();

/// 全局事件总线访问器。
///
/// 提供全局单例模式的 EventBus 访问。
/// 必须在应用启动时调用 [`GlobalEventBus::initialize`] 进行初始化。
pub struct GlobalEventBus;

impl GlobalEventBus {
    /// 初始化全局事件总线。
    ///
    /// # Errors
    ///
    /// 如果全局事件总线已经初始化，返回 [`TraitError::AlreadyInitialized`]。
    pub fn initialize() -> Result<(), TraitError> {
        GLOBAL_EVENT_BUS
            .set(EventBus::new())
            .map_err(|_| TraitError::AlreadyInitialized("全局事件总线已初始化".to_string()))
    }

    /// 获取全局事件总线引用。
    ///
    /// # Panics
    ///
    /// 如果全局事件总线未初始化，将 panic。
    pub fn get() -> &'static EventBus {
        GLOBAL_EVENT_BUS
            .get()
            .expect("全局事件总线未初始化，请先调用 GlobalEventBus::initialize()")
    }

    /// 检查全局事件总线是否已初始化。
    ///
    /// # Returns
    ///
    /// 已初始化返回 `true`，否则返回 `false`。
    pub fn is_initialized() -> bool {
        GLOBAL_EVENT_BUS.get().is_some()
    }
}
