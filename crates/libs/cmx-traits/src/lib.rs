//! cmx-traits — 跨模块 trait 接口抽象层
//!
//! 本 crate 定义了 cmx-container 项目中所有跨模块交互的 trait 接口，
//! 作为模块间解耦的核心枢纽。
//!
//! # 设计目标
//!
//! - 各业务模块（cmx-plugin, cmx-service, cmx-runtime）仅依赖本 crate 的 trait 定义
//! - 模块间通过 trait 对象交互，不直接依赖彼此的 crate
//! - 支持依赖注入和单元测试 mock
//!
//! # 核心接口
//!
//! - [`PluginQuery`] — 插件状态查询（cmx-service 查询 cmx-plugin）
//! - [`RuntimeInvoker`] — WASM 运行时调用（cmx-service 调用 cmx-runtime）
//! - [`PluginLifecycleListener`] — 生命周期事件监听（cmx-plugin 通知 cmx-service）
//! - [`ExtismFunctionProvider`] — 宿主函数注册（各模块向 cmx-runtime 注册宿主函数）

// 模块声明
pub mod plugin_query;
pub mod runtime_invoker;
pub mod lifecycle;
pub mod host_func;
pub mod error;
pub mod global_runtime;
pub mod invoke_context;
pub mod service_query;
pub mod service_storage;

// 统一导出
pub use plugin_query::{PluginQuery, PluginSnapshot, PluginFilter};
pub use runtime_invoker::{RuntimeInvoker, WasmInvokeResult};
pub use lifecycle::{PluginLifecycleListener, LifecycleEvent};
pub use host_func::{HostFunctionProvider, HostFunctionDef, ValType};
pub use error::{TraitError, HostFuncError};
pub use global_runtime::GlobalRuntime;
pub use invoke_context::{InvokeOptions, InvokeContext, InvokeGuard, InvokeGuardError, DEFAULT_TIMEOUT, DEFAULT_MAX_DEPTH};
pub use service_query::ServiceQuery;
pub use service_storage::ServiceStorage;
