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

// 重新导出所有类型，方便外部使用
pub use database::{DbQueryRequest, DbResponse};
pub use cache::{CacheGetRequest, CacheSetRequest, CacheResponse};
pub use plugin::{ServiceCallRequest, ServiceCallResponse, PluginInfoResponse};
pub use context::WasmContext;
pub use common::{WasmFunctionRequest, WasmFunctionResponse};
