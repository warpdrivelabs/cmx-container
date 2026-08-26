//! WASM 类型定义模块
//!
//! 定义宿主与 WASM 之间交互的所有数据结构。
//!
//! # 模块结构
//!
//! - `database`: 数据库操作类型
//! - `cache`: 缓存操作类型
//! - `http`: HTTP 出站操作类型（`cmx:http` 宿主函数，W4）
//! - `plugin`: 插件调用类型
//! - `context`: WASM 上下文类型
//! - `common`: 通用包装类型
//! - `iam`: 用户/权限查询类型（`cmx:iam` 宿主函数）

pub mod cache;
pub mod common;
pub mod context;
pub mod database;
pub mod execution;
pub mod http;
pub mod iam;
pub mod plugin;

pub use cache::{CacheGetRequest, CacheResponse, CacheSetRequest};
pub use common::{WasmFunctionRequest, WasmFunctionResponse};
pub use context::WasmContext;
pub use database::{DbRequest, DbResponse};
pub use execution::{ExecutionStep, OrchestrationError, StepStatus};
pub use http::{HttpRequest, HttpResponse};
pub use iam::{
    IamRequest, IamResponse, WasmCheckResult, WasmEffectivePermissions, WasmUserDetails,
};
pub use plugin::{
    CallServiceRequest, CallServiceResponse, PluginFunCallResponse, PluginFunRequest,
    PluginInfoResponse,
};
