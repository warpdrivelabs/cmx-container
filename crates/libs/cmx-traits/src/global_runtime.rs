//! 全局运行时存储器
//!
//! 提供全局访问 RuntimeInvoker 的能力。
//! 由于 Extism 的 host_fn! 宏不支持捕获外部变量，
//! 需要通过全局静态变量来访问运行时实例。

use std::sync::OnceLock;

use crate::runtime_invoker::RuntimeInvoker;

/// 全局运行时错误类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GlobalRuntimeError(&'static str);

impl GlobalRuntimeError {
    pub const ALREADY_SET: Self = GlobalRuntimeError("运行时已初始化，无法重复设置");
}

/// 全局运行时存储器
///
/// 用于在宿主函数中访问 WASM 运行时实例。
/// 在应用启动时通过 `set` 方法设置，之后可通过 `get` 方法获取。
pub struct GlobalRuntime;

static RUNTIME: OnceLock<std::sync::Arc<dyn RuntimeInvoker>> = OnceLock::new();

impl GlobalRuntime {
    /// 设置全局运行时实例
    ///
    /// # 参数
    /// - `runtime`: WASM 运行时实例
    ///
    /// # 返回值
    /// - `Ok(())`: 设置成功
    /// - `Err(GlobalRuntimeError)`: 已设置过，无法重复设置
    #[allow(clippy::result_unit_err)]
    pub fn set(runtime: std::sync::Arc<dyn RuntimeInvoker>) -> Result<(), GlobalRuntimeError> {
        RUNTIME.set(runtime).map_err(|_| GlobalRuntimeError::ALREADY_SET)
    }

    /// 获取全局运行时实例
    ///
    /// # Panics
    /// 如果未初始化则 panic
    pub fn get() -> &'static std::sync::Arc<dyn RuntimeInvoker> {
        RUNTIME.get().expect("GlobalRuntime 未初始化，请先调用 GlobalRuntime::set()")
    }

    /// 检查是否已初始化
    pub fn is_initialized() -> bool {
        RUNTIME.get().is_some()
    }
}
