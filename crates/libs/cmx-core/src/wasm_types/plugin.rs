/*
 * @Author: yqs
 * @Date: 2026-04-13 22:20:01
 * @Describe:
 * @LastEditors: yqs
 * @LastEditTime: 2026-05-06 14:17:24
 */
//! 插件调用相关类型
//!
//! 定义宿主与 WASM 之间插件调用的请求和响应结构体。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::execution::{ExecutionStep, OrchestrationError};

/// 插件信息响应
///
/// 宿主返回给 WASM 插件的当前插件信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfoResponse {
    /// 当前插件ID
    pub plugin_id: String,
    /// 数据库ID
    pub db_id: String,
    /// 当前事务ID
    pub txn_id: Option<String>,
    /// 请求ID
    pub request_id: String,
    /// 租户ID
    pub tenant_id: Option<String>,
}

/// 调用指定插件的指定函数请求
///
/// 用于 WASM 插件通过宿主函数调用另一个插件的指定函数。
/// 类似于 API `/api/service/call` 的功能，但运行在 WASM 插件上下文中。
///
/// # 字段说明
/// - `plugin_id`: 目标插件的唯一标识
/// - `function_name`: 目标插件中要调用的函数名
/// - `input`: 传递给函数的输入数据（JSON 格式，支持任意结构）
/// - `initial_input`: 初始输入数据（可选，用于调试场景）
/// - `debug`: 是否启用调试模式（可选）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginFunRequest {
    /// 目标插件ID
    pub plugin_id: String,
    /// 目标函数名
    pub function_name: String,
    /// 传递给函数的输入数据（JSON 格式）
    pub input: serde_json::Value,
    /// 初始输入数据（调试时传递服务最开始的入参，可选）
    pub initial_input: Option<serde_json::Value>,
    /// 是否启用调试模式（可选，默认 false）
    pub debug: Option<bool>,
    /// 目标服务名称（跨服务调用时指定，不指定则本地调用）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_name: Option<String>,
}

/// 调用指定服务的请求
///
/// 用于 WASM 插件通过宿主函数执行一个完整的服务编排。
/// 类似于 API `/api/service/execute` 的功能，但运行在 WASM 插件上下文中。
///
/// # 字段说明
/// - `service_key`: 服务的唯一标识（对应服务.json 中的 code 字段）
/// - `input`: 传递给第一个函数节点的输入数据（JSON 格式）
/// - `include_steps`: 是否返回各步骤的执行详情（可选，默认 false）
/// - `debug`: 是否启用调试模式（可选，默认 false）
/// - `debug_node_id`: 调试目标节点ID（启用 debug 时必填）
/// - `debug_params`: 调试参数（可选，HashMap 形式）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallServiceRequest {
    /// 服务唯一标识（对应服务.json 中的 code 字段）
    pub service_key: String,
    /// 传递给第一个函数节点的输入数据（JSON 格式）
    pub input: serde_json::Value,
    /// 是否返回各步骤的执行详情（可选，默认 false）
    pub include_steps: Option<bool>,
    /// 是否启用调试模式（可选，默认 false）
    pub debug: Option<bool>,
    /// 调试目标节点ID（启用 debug 时必填）
    pub debug_node_id: Option<String>,
    /// 调试参数（可选，用于传递额外的调试配置）
    pub debug_params: Option<HashMap<String, String>>,
    /// 目标服务名称（跨服务调用时指定，不指定则本地调用）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_name: Option<String>,
}

/// 插件函数调用响应
///
/// 宿主函数返回给 WASM 插件的单次函数调用结果。
/// 包含执行状态、函数输出和耗时。
///
/// # 字段说明
/// - `success`: 是否执行成功
/// - `result`: 函数执行结果（JSON 格式，失败时为 None）
/// - `elapsed_us`: 执行耗时（微秒）
/// - `error`: 错误信息（成功时为 None）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginFunCallResponse {
    /// 是否执行成功
    pub success: bool,
    /// 函数执行结果（JSON 格式，失败时为 None）
    pub result: Option<serde_json::Value>,
    /// 执行耗时（微秒）
    pub elapsed_us: Option<u64>,
    /// 错误信息（成功时为 None）
    pub error: Option<String>,
}

/// 服务调用响应
///
/// 宿主函数返回给 WASM 插件的调用结果。
/// 包含执行状态、输出、步骤详情、耗时和错误信息。
///
/// # 字段说明
/// - `success`: 是否执行成功
/// - `output`: 执行结果（JSON 格式，失败时为 None）
/// - `steps`: 各步骤执行记录（成功时为空数组，除非 include_steps=true）
/// - `elapsed_us`: 执行耗时（微秒）
/// - `error`: 错误信息（成功时为 None）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallServiceResponse {
    /// 是否执行成功（所有节点都成功执行则为 true）
    pub success: bool,
    /// 最终输出结果（最后一个节点的输出，失败时为 None）
    pub output: Option<serde_json::Value>,
    /// 各步骤执行记录（按执行顺序记录每个节点的执行情况）
    /// 注意：当 include_steps=false 且成功时，此数组为空
    pub steps: Vec<ExecutionStep>,
    /// 总执行耗时（微秒）
    pub total_elapsed_us: Option<u64>,
    /// 结构化错误信息（失败时包含错误摘要）
    pub error: Option<OrchestrationError>,
}
