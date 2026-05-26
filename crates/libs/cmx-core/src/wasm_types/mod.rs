//! WASM 类型定义模块
//!
//! 定义宿主与 WASM 之间交互的所有数据结构。
//!
//! # 模块结构
//!
//! - `database`: 数据库操作类型
//! - `cache`: 缓存操作类型
//! - `plugin`: 插件调用类型
//! - `context`: WASM 上下文类型
//! - `common`: 通用包装类型

pub mod database;
pub mod cache;
pub mod plugin;
pub mod context;
pub mod common;
pub mod execution;

pub use database::{DbRequest, DbResponse};
pub use cache::{CacheGetRequest, CacheSetRequest, CacheResponse};
pub use plugin::{ PluginInfoResponse, PluginFunRequest, PluginFunCallResponse, CallServiceRequest, CallServiceResponse};
pub use context::WasmContext;
pub use common::{WasmFunctionRequest, WasmFunctionResponse};
pub use execution::{ExecutionStep, StepStatus, OrchestrationError};
