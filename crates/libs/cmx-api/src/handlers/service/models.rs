//! 服务 API 请求/响应结构体定义
//!
//! 包含服务相关 API 的请求和响应数据结构。

use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

// ==================== 函数直接调用请求/响应结构体 ====================

/// 函数调用请求
///
/// 用于 POST /api/service/call 接口，直接调用指定插件的函数。
///
/// # 统一入参说明
///
/// 所有 WASM 函数都使用 `FunctionInput` 作为入参，包含：
/// - `input`: 当前步骤输入数据（JSON 字符串或纯文本）
/// - `context`: 服务调用上下文（初始入参、请求头、步骤输出、事务ID）
/// - `binary_data`: 二进制数据（文件、图像等）
///
/// 本请求体中的 `input` 会被封装到 `FunctionInput` 中传递给函数。
/// `headers` 从 HTTP 请求头中获取，不从请求体传递。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FunctionCallRequest {
    /// 插件ID（要调用的插件）
    pub plugin_id: String,
    /// 函数名（要调用的函数）
    pub function_name: String,
    /// 输入数据（传递给函数的 input 字段，支持 JSON 对象或字符串）
    pub input: serde_json::Value,
    /// 初始输入数据（调试的时候传服务最开始的参数）
    pub initial_input: Option<serde_json::Value>,
    /// 是否调试模式
    #[serde(default)]
    pub debug:bool,
    /// 目标服务名称（跨服务调用时指定，不指定则本地调用）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_name: Option<String>,
}

/// 函数调用响应
///
/// 包含函数执行的完整结果。
///
/// # 统一出参说明
///
/// 所有 WASM 函数都使用 `FunctionOutput` 作为出参，包含：
/// - `result`: 函数执行结果
/// - `binary_data`: 二进制数据（文件、图像等）
///
/// 本响应体中的 `result` 来自 `FunctionOutput.result`。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FunctionCallResponse {
    /// 是否执行成功
    pub success: bool,
    /// 函数执行结果（来自 FunctionOutput.result）
    pub result: Option<serde_json::Value>,
    /// 执行耗时（微秒）
    pub elapsed_us: u64,
    /// 错误信息（失败时）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

// ==================== 服务编排请求/响应结构体 ====================

/// 服务执行请求
///
/// 用于 POST /api/service/execute 接口。
///
/// # 说明
/// `headers` 从 HTTP 请求头中获取，不从请求体传递。
/// `input` 支持 JSON 对象或字符串。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ServiceExecuteRequest {
    /// 服务唯一标识（对应 服务.json 中的 code 字段）
    pub service_key: Option<String>,
    /// 初始输入数据（传递给第一个函数节点，支持 JSON 对象或字符串）
    pub input: serde_json::Value,
    /// 是否返回步骤数据（可选，默认 false）
    /// - true: 调试模式，返回每个步骤的详细数据
    /// - false: 生产模式，仅返回最终结果
    /// - 注意：执行失败时无论此参数设置如何，都会返回步骤数据
    #[serde(default)]
    pub include_steps: Option<bool>,
    /// 是否开启调试模式（可选，默认 false）
    #[serde(default)]
    pub debug: Option<bool>,
    /// 调试目标节点ID（开启 debug 时必填）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug_node_id: Option<String>,

    /// 调试参数（开启 debug 时有）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug_params: Option<HashMap<String, String>>,

    /// 目标服务名称（跨服务调用时指定，不指定则本地调用）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_name: Option<String>,

}

/// 服务执行响应
///
/// 包含服务编排执行的完整结果
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ServiceExecuteResponse {
    /// 是否执行成功（所有节点都成功执行则为 true）
    pub success: bool,
    /// 最终输出结果（最后一个节点的输出，失败时为 None）
    pub output: Option<serde_json::Value>,
    /// 各步骤执行记录（按执行顺序）
    pub steps: Vec<ServiceExecutionStep>,
    /// 总执行耗时（微秒）
    pub total_elapsed_us: u64,
    /// 错误详情（失败时包含结构化错误信息）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ServiceOrchestrationError>,
    /// 是否触发了调试暂停
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug_triggered: Option<bool>,
    /// 调试准备结果（触发调试暂停时包含调试信息）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug_prepare_result: Option<ServiceDebugPrepareResult>,
}

/// 服务执行步骤记录
///
/// 记录单个节点的执行情况
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ServiceExecutionStep {
    /// 节点ID（对应 Flow JSON 中的 node.id）
    pub node_id: String,
    /// 节点名称（对应 Flow JSON 中的 node.data.name）
    pub node_name: String,
    /// 节点类型（如 skylake-func、skylake-switch）
    pub node_type: String,
    /// 步骤执行状态
    pub status: String,
    /// 步骤输出（函数执行结果，失败时可能为 None）
    pub output: Option<serde_json::Value>,
    /// 执行耗时（微秒）
    pub elapsed_us: u64,
    /// 步骤级错误信息（失败时包含具体错误描述）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// 上一步的输出（失败时便于排错，记录失败前的数据上下文）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_output: Option<serde_json::Value>,
}
///
/// 失败时提供错误摘要信息
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ServiceOrchestrationError {
    /// 错误摘要信息
    pub message: String,
}

/// 调试准备结果
///
/// 当编排执行到调试目标节点时，返回给前端的调试准备信息。
/// 包含插件详细信息、code-server 在线编辑器 URL 和节点信息，
/// 供前端打开 code-server 编辑器并定位到待调试的插件源码。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ServiceDebugPrepareResult {
    /// code-server 在线编辑器 URL
    pub code_server_url: String,
    /// 插件唯一标识
    pub plugin_id: String,
    /// 插件名称
    pub plugin_name: String,
    /// 插件版本
    pub plugin_version: String,
    /// 插件状态（installed/activated/deactivated/error）
    pub plugin_status: String,
    /// 插件安装路径（绝对路径）
    pub plugin_install_path: String,
    /// WASM 文件路径（相对于安装路径）
    pub plugin_wasm_path: Option<String>,
    /// 插件类型（wasm/rhai）
    pub plugin_type: String,
    /// 插件所属域编码
    pub domain_code: String,
    /// 插件所属应用编码
    pub application_code: String,
    /// 插件所属模块编码
    pub module_code: String,
    /// 要调试的函数名称
    pub function_name: String,
    /// 插件源码路径（从 manifest.json 读取）
    pub source_path: Option<String>,
    /// 调试目标节点ID
    pub node_id: String,
    /// 调试目标节点名称
    pub node_name: String,
}

// ==================== 服务查询请求结构体 ====================

/// 服务获取查询参数
///
/// 用于 GET /api/service/get 接口
#[derive(Debug, Clone, Serialize, Deserialize, IntoParams)]
pub struct ServiceGetQuery {
    /// 服务唯一标识
    pub service_key: String,
}

/// 插件服务查询参数
///
/// 用于 GET /api/service/by-plugin 接口
#[derive(Debug, Clone, Serialize, Deserialize, IntoParams)]
pub struct ServiceByPluginQuery {
    /// 插件ID
    pub plugin_id: String,
}

// ==================== 服务列表响应结构体 ====================

/// 服务列表项
///
/// 用于 GET /api/service/list 和 GET /api/service/by-plugin 接口的响应
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ServiceListItem {
    /// 主键ID
    pub id: String,
    /// 服务唯一标识
    pub service_key: String,
    /// 服务名称
    pub service_name: String,
    /// 服务描述
    pub description: String,
    /// 所属插件ID
    pub plugin_id: String,
    /// 状态
    pub status: i32,
    /// 当前版本
    pub version: String,
    /// 所属域代码
    pub domain_code: String,
    /// 所属应用代码
    pub application_code: String,
    /// 所属模块代码
    pub module_code: String,
    /// 所属域名称
    pub domain_name: String,
    /// 所属应用名称
    pub application_name: String,
    /// 所属模块名称
    pub module_name: String,
    /// 插件名称
    pub plugin_name: String,
    /// 服务api文档
    pub api_doc: Option<String>,
}

/// 服务查重查询参数
#[derive(Debug, Clone, Serialize, Deserialize, IntoParams)]
pub struct ServiceExistsQuery {
    /// 服务唯一标识
    pub service_key: String,
}



// ==================== 服务删除请求结构体 ====================

/// 服务删除请求
///
/// 用于 POST /api/service/delete 接口
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ServiceDeleteQuery {
    /// 服务唯一标识
    pub service_key: String,
}

/// 服务详情响应
///
/// 用于 GET /api/service/get 接口的响应
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ServiceDetailResponse {
    /// 主键ID
    pub id: String,
    /// 服务唯一标识
    pub service_key: String,
    /// 服务名称
    pub service_name: String,
    /// 服务描述
    pub description: String,
    /// 所属插件ID
    pub plugin_id: String,
    /// 状态
    pub status: i32,
    /// 当前版本
    pub version: String,
    //服务编排 配置
    pub  config: String,
    /// 所属域代码
    pub domain_code: String,
    /// 所属应用代码
    pub application_code: String,
    /// 所属模块代码
    pub module_code: String,
    /// 所属域名称
    pub domain_name: String,
    /// 所属应用名称
    pub application_name: String,
    /// 所属模块名称
    pub module_name: String,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct OpenApiQuery {
    /// 是否强制刷新缓存
    #[serde(default)]
    pub refresh: Option<bool>,
    /// 插件 ID 精确匹配
    #[serde(default)]
    pub plugin_id: Option<String>,
    /// 域代码精确匹配
    #[serde(default)]
    pub domain_code: Option<String>,
    /// 应用代码精确匹配
    #[serde(default)]
    pub application_code: Option<String>,
    /// 模块代码精确匹配
    #[serde(default)]
    pub module_code: Option<String>,
}
