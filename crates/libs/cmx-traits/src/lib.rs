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
//! # 模块组织
//!
//! - [`error`] — 通用错误类型（TraitError, HostFuncError）
//! - [`auth`] — 认证领域（AuthService, AuthPolicy, UserAuthQuery, AuthStorageQuery）
//! - [`iam`] — IAM 领域（PermissionChecker, DataScope）
//! - [`plugin`] — 插件领域（PluginQuery, PluginLifecycleListener）
//! - [`runtime`] — WASM 运行时领域（RuntimeInvoker, HostFunctionProvider, InvokeContext）
//! - [`service`] — 服务领域（ServiceQuery, ServiceStorage, ServiceInvoker）
//! - [`rpc`] — RPC 领域（RpcClient）
//! - [`event_bus`] — 事件总线（EventBus, GlobalEventBus）

// 模块声明
pub mod error;
pub mod auth;
pub mod iam;
pub mod plugin;
pub mod runtime;
pub mod service;
pub mod rpc;
pub mod event_bus;
