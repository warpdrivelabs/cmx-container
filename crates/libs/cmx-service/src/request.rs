//! 请求/响应类型定义
//!
//! 定义 cmx-service 对外暴露的请求和响应结构体。

use serde::{Deserialize, Serialize};

/// 单次调用请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvokeRequest {
    /// 目标插件ID
    pub plugin_id: String,
    /// 目标函数名
    pub function_name: String,
    /// 输入数据（JSON 值）
    pub input: serde_json::Value,
    /// 数据库ID（可选，默认使用系统数据库）
    pub db_id: Option<String>,
    /// 请求ID（可选，用于追踪）
    pub request_id: Option<String>,
    /// 租户ID（可选，用于多租户场景）
    pub tenant_id: Option<String>,
}

/// 单次调用响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvokeResponse {
    /// 是否成功
    pub success: bool,
    /// 输出数据（JSON 值）
    pub output: Option<serde_json::Value>,
    /// 执行耗时（微秒）
    pub elapsed_us: u64,
    /// 消耗的 fuel
    pub fuel_consumed: u64,
    /// 错误信息
    pub error: Option<String>,
}

/// 编排执行请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrateRequest {
    /// 编排定义（内联）
    pub orchestration: super::orchestrator::Orchestration,
    /// 初始输入数据
    pub initial_input: serde_json::Value,
    /// 数据库ID
    pub db_id: Option<String>,
    /// 请求ID
    pub request_id: Option<String>,
    /// 租户ID
    pub tenant_id: Option<String>,
}

/// 编排执行响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrateResponse {
    /// 是否成功
    pub success: bool,
    /// 最终输出数据
    pub final_output: Option<serde_json::Value>,
    /// 各步骤执行结果
    pub step_results: Vec<StepResult>,
    /// 总执行耗时（微秒）
    pub total_elapsed_us: u64,
    /// 错误信息
    pub error: Option<String>,
}

/// 单个步骤执行结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult {
    /// 步骤ID
    pub step_id: String,
    /// 是否成功
    pub success: bool,
    /// 输出数据
    pub output: Option<serde_json::Value>,
    /// 执行耗时（微秒）
    pub elapsed_us: u64,
    /// 错误信息
    pub error: Option<String>,
}
