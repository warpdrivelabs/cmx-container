//! 全局服务调用器存储器
//!
//! 提供全局访问 ServiceInvoker 的能力。
//! 由于 Extism 的 host_fn! 宏不支持捕获外部变量，
//! 需要通过全局静态变量来访问服务调用器实例。

use std::sync::{Arc, OnceLock};

use crate::service_invoker::ServiceInvoker;

/// 全局服务调用器错误类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GlobalServiceInvokerError(&'static str);

impl GlobalServiceInvokerError {
    pub const ALREADY_SET: Self = GlobalServiceInvokerError("服务调用器已初始化，无法重复设置");
}

/// 全局服务调用器存储器
///
/// 用于在宿主函数中访问服务编排执行能力。
/// 在应用启动时通过 `set` 方法设置，之后可通过 `get` 方法获取。
pub struct GlobalServiceInvoker;

static SERVICE_INVOKER: OnceLock<Arc<dyn ServiceInvoker>> = OnceLock::new();

impl GlobalServiceInvoker {
    /// 设置全局服务调用器实例
    pub fn set(invoker: Arc<dyn ServiceInvoker>) -> Result<(), GlobalServiceInvokerError> {
        SERVICE_INVOKER
            .set(invoker)
            .map_err(|_| GlobalServiceInvokerError::ALREADY_SET)
    }

    /// 获取全局服务调用器实例
    ///
    /// # Panics
    /// 如果未初始化则 panic
    pub fn get() -> &'static Arc<dyn ServiceInvoker> {
        SERVICE_INVOKER
            .get()
            .expect("GlobalServiceInvoker 未初始化，请先调用 GlobalServiceInvoker::set()")
    }

    /// 检查是否已初始化
    pub fn is_initialized() -> bool {
        SERVICE_INVOKER.get().is_some()
    }
}
