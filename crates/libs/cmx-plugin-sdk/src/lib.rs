//! cmx-plugin-sdk — CMX 插件开发 SDK
//!
//! 基于 Extism PDK 的插件开发 SDK，提供：
//! - 宿主函数调用封装（需启用 extism feature）
//! - 错误类型定义
//! - 标准入参出参类型
//!
//! # Feature 说明
//!
//! - `extism`（默认启用）：启用 Extism PDK 集成，包括宿主函数调用和插件导出宏
//! - 关闭此 feature 后，SDK 仅提供类型定义，适用于纯业务逻辑的测试和调试
//!
//! # 标准函数签名（extism feature 启用时）
//!
//! 所有服务编排中的函数都应该使用统一的入参和出参格式：
//!
//! ```rust
//! use cmx_plugin_sdk::{FunctionInput, FunctionOutput, SVRContext};
//! use extism_pdk::*;
//!
//! #[plugin_fn]
//! pub fn my_function(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
//!     let current_input = &input.input;
//!     let initial_input = &input.context.initial_input;
//!     let headers = &input.context.headers;
//!     if let Some(prev_output) = input.context.get_step_output("previous_node_id") {}
//!     Ok(Msgpack(FunctionOutput::from_json(serde_json::json!({
//!         "status": "success",
//!         "data": "处理结果"
//!     }))))
//! }
//! ```

pub mod error;

#[cfg(feature = "extism")]
pub mod host_calls;

// ==================== Extism PDK 导出 ====================

#[cfg(feature = "extism")]
pub use extism_pdk::*;

// ==================== 宿主函数调用 ====================

#[cfg(feature = "extism")]
pub use host_calls::HostCaller;

// ==================== WASM 类型（从 cmx_core 导出） ====================

pub use cmx_core::{
    DbRequest, DbResponse,
    CacheGetRequest, CacheSetRequest, CacheResponse,
    PluginFunRequest, PluginFunCallResponse, CallServiceRequest, CallServiceResponse,
    ExecutionStep, StepStatus, OrchestrationError,
};

// ==================== 错误类型 ====================

pub use error::PluginError;

// ==================== 标准入参出参类型 ====================

pub use cmx_core::FunctionInput;

pub use cmx_core::FunctionOutput;

pub use cmx_core::SVRContext;
