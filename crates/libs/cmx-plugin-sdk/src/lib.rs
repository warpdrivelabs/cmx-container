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
//!     // 获取当前步骤输入（字符串）
//!     let current_input = input.as_str();
//!     
//!     // 解析为 JSON（如果输入是 JSON 格式）
//!     let json_value = input.as_json_value();
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
//!     // 返回结果（使用辅助方法）
//!     Ok(Json(FunctionOutput::from_json(serde_json::json!({
//!         "status": "success",
//!         "data": "处理结果"
//!     }))))
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
/// - `input`: 当前步骤输入数据（JSON 字符串或纯文本）
/// - `context`: 服务调用上下文，包含初始入参、请求头、各步骤输出、事务ID
/// - `binary_data`: 二进制数据（文件、图像等）
///
/// # 辅助方法
///
/// - `as_str()`: 获取输入作为字符串
/// - `as_json_value()`: 解析为 JSON Value（宽松模式）
/// - `parse_json::<T>()`: 解析为指定类型
///
/// # 示例
///
/// ```rust
/// use cmx_plugin_sdk::FunctionInput;
/// use std::collections::HashMap;
///
/// let input = FunctionInput {
///     input: r#"{"name":"test"}"#.to_string(),
///     context: SVRContext::new("初始入参".to_string(), HashMap::new()),
///     binary_data: HashMap::new(),
/// };
///
/// // 使用辅助方法
/// let json = input.as_json_value();
/// let name = input.parse_json::<serde_json::Value>();
/// ```
pub use cmx_core::FunctionInput;

/// 函数输出结构体 — 固定出参格式
///
/// 所有服务编排中的函数都应该使用此结构体作为出参。
///
/// # 字段说明
///
/// - `result`: 函数执行结果（JSON 字符串或纯文本）
/// - `binary_data`: 二进制数据（文件、图像等）
///
/// # 辅助方法
///
/// - `new(result)`: 从字符串创建输出
/// - `from_json(value)`: 从 JSON Value 创建输出
/// - `with_binary(key, data)`: 添加二进制数据
///
/// # 示例
///
/// ```rust
/// use cmx_plugin_sdk::FunctionOutput;
///
/// // 从字符串创建
/// let output = FunctionOutput::new("处理结果");
///
/// // 从 JSON 创建
/// let output = FunctionOutput::from_json(serde_json::json!({
///     "status": "success"
/// }));
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
