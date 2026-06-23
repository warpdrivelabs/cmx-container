//! WASM 运行时领域 trait 抽象。
//!
//! 包含运行时调用、宿主函数注册、调用上下文管理、全局运行时存储等接口。
//!
//! # 模块组织
//!
//! - [`invoker`] — WASM 运行时调用 trait（RuntimeInvoker）。
//! - [`host_func`] — 宿主函数提供者 trait（HostFunctionProvider）。
//! - [`invoke_context`] — 调用上下文与深度/循环检测（InvokeContext）。
//! - [`global`] — 全局运行时存储器（GlobalRuntime）。

pub mod invoker;
pub mod host_func;
pub mod invoke_context;
pub mod global;

pub use invoker::{RuntimeInvoker, WasmInvokeResult};
pub use host_func::{HostFunctionProvider, HostFunctionDef, ValType};
pub use invoke_context::{
    InvokeOptions, InvokeContext, InvokeGuard, InvokeGuardError, DEFAULT_TIMEOUT, DEFAULT_MAX_DEPTH,
};
pub use global::GlobalRuntime;
