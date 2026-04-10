//! cmx-plugin-sdk — CMX 插件开发 SDK
//!
//! 基于 Extism PDK 的插件开发 SDK，提供：
//! - 宿主函数调用封装
//! - 插件函数导出宏
//! - 错误类型定义
//! - 标准入参出参类型
//!
//! # 标准函数签名
//!
//! 所有服务编排中的函数都应该使用统一的入参和出参格式：
//!
//! ```rust
//! use cmx_plugin_sdk::{FunctionInput, FunctionOutput, SVRContext};
//! use extism_pdk::*;
//!
//! #[plugin_fn]
//! pub fn my_function(Json(input): Json<FunctionInput>) -> FnResult<Json<FunctionOutput>> {
//!     // 获取当前步骤输入
//!     let current_input = &input.input;
//!     
//!     // 获取初始入参
//!     let initial_input = &input.context.initial_input;
//!     
//!     // 获取请求头
//!     let headers = &input.context.headers;
//!     
//!     // 获取前序步骤输出
//!     if let Some(prev_output) = input.context.get_step_output("previous_node_id") {
//!         // 使用前序步骤输出
//!     }
//!     
//!     // 返回结果
//!     Ok(Json(FunctionOutput {
//!         result: "处理结果".to_string(),
//!     }))
//! }
//! ```

pub mod host_calls;
pub mod error;

// ==================== Extism PDK 导出 ====================

pub use extism_pdk::*;

// ==================== 宿主函数调用 ====================

pub use host_calls::{
    HostCaller,
    DbQueryRequest, DbResponse,
    CacheGetRequest, CacheSetRequest, CacheResponse,
    ServiceCallRequest, ServiceCallResponse,
};

// ==================== 错误类型 ====================

pub use error::PluginError;

// ==================== 标准入参出参类型 ====================

/// 函数输入结构体 — 固定入参格式
///
/// 所有服务编排中的函数都应该使用此结构体作为入参。
///
/// # 字段说明
///
/// - `input`: 当前步骤输入数据（前序步骤输出或初始输入）
/// - `context`: 服务调用上下文，包含初始入参、请求头、各步骤输出
/// - `txn_id`: 事务ID（仅在事务框内执行时设置）
///
/// # 示例
///
/// ```rust
/// use cmx_plugin_sdk::FunctionInput;
///
/// let input = FunctionInput {
///     input: "当前步骤输入".to_string(),
///     context: SVRContext::new("初始入参".to_string(), Default::default()),
///     txn_id: None,
/// };
/// ```
pub use cmx_core::FunctionInput;

/// 函数输出结构体 — 固定出参格式
///
/// 所有服务编排中的函数都应该使用此结构体作为出参。
///
/// # 字段说明
///
/// - `result`: 函数执行结果，将传递给下一个步骤
///
/// # 示例
///
/// ```rust
/// use cmx_plugin_sdk::FunctionOutput;
///
/// let output = FunctionOutput {
///     result: "处理结果".to_string(),
/// };
/// ```
pub use cmx_core::FunctionOutput;

/// 服务调用上下文 — 在函数间传递
///
/// 包含服务调用的完整上下文信息，在编排执行过程中持续传递。
///
/// # 字段说明
///
/// - `initial_input`: 初始调用入参（API 请求传入的参数）
/// - `headers`: HTTP 请求头信息
/// - `step_outputs`: 各步骤执行结果的缓存（步骤ID -> 输出）
///
/// # 示例
///
/// ```rust
/// use cmx_plugin_sdk::SVRContext;
/// use std::collections::HashMap;
///
/// // 创建上下文
/// let mut context = SVRContext::new(
///     "初始入参".to_string(),
///     HashMap::new(),
/// );
///
/// // 添加步骤输出
/// context.add_step_output("node_1".to_string(), "步骤1结果".to_string());
///
/// // 获取步骤输出
/// if let Some(output) = context.get_step_output("node_1") {
///     println!("步骤1输出: {}", output);
/// }
/// ```
pub use cmx_core::SVRContext;
