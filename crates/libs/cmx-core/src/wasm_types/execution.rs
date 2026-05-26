//! 编排执行相关类型
//!
//! 定义服务编排执行过程中的步骤记录、状态枚举和错误信息。
//! 这些类型在宿主侧和 WASM 侧之间共享。

use serde::{Deserialize, Serialize};

/// 执行步骤状态枚举
///
/// 用于标识每个编排步骤的执行结果状态。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum StepStatus {
    /// 执行成功 — 节点函数正常返回
    Success,
    /// 执行失败 — 节点函数抛出异常或返回错误
    Failed,
    /// 跳过 — 节点被跳过未执行（如条件分支未命中）
    Skipped,
    /// 调试暂停 — 节点被调试模式拦截，未实际执行
    DebugPaused,
}

/// 执行步骤记录
///
/// 记录单个节点的执行情况，包括状态、输出和耗时。
/// 每个步骤对应 Flow JSON 中的一个节点执行结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionStep {
    /// 节点ID（对应 Flow JSON 中的 node.id）
    pub node_id: String,
    /// 节点名称（对应 Flow JSON 中的 node.data.name，用于日志和调试）
    pub node_name: String,
    /// 节点类型（如 skylake-func、skylake-switch、skylake-transaction）
    pub node_type: String,
    /// 步骤执行状态（Success/Failed/Skipped）
    pub status: StepStatus,
    /// 步骤输出（函数执行结果，失败时可能为 None）
    pub output: Option<serde_json::Value>,
    /// 执行耗时（微秒，用于性能分析）
    pub elapsed_us: u64,
    /// 步骤级错误信息（失败时包含具体错误描述，成功时为 None）
    pub error: Option<String>,
    /// 上一步的输出（失败时便于排错，记录失败前的数据上下文）
    /// 成功时为 None，序列化时跳过
    pub previous_output: Option<serde_json::Value>,
}

/// 编排错误信息
///
/// 失败时提供错误摘要信息，失败步骤的详细信息（包括 previous_output）
/// 统一记录在 steps 数组中对应步骤的 ExecutionStep 里。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationError {
    /// 错误摘要信息（人类可读的错误描述，适合展示给用户）
    pub message: String,
}
