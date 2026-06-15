//! 服务编排执行核心逻辑
//!
//! 提取 cmx-api（HTTP）中通过 `Orchestrator` 执行服务编排的核心链路，
//! 实现协议无关的统一执行入口。包含完整调用链：
//! 构造 Orchestrator → 执行编排 → 映射结果（含 StepStatus 转字符串）。
//!
//! # 设计说明
//!
//! 本模块只负责"执行"环节，不涉及：
//! - 参数提取（由协议层从 HTTP/protobuf 中解析）
//! - ExecuteOptions 构建（由协议层组装）
//! - SVRContext 构建（由协议层从 middleware 或请求中组装）
//! - 响应封装（由协议层转换为 JSON/protobuf 响应）
//!
//! # 调用结果处理
//!
//! - 基础设施错误（服务未找到、编排配置缺失等）通过 `Err(BizError)` 返回
//! - 节点执行失败通过 `Ok(ServiceExecuteCoreResult { success: false, ... })` 返回，
//!   由调用方决定如何映射为协议级错误或业务级失败响应

use std::sync::Arc;

use cmx_core::model::service::SVRContext;
use cmx_core::{ExecutionStep, OrchestrationError, StepStatus};
use cmx_service::{DebugPrepareResult, ExecuteOptions};
use cmx_traits::{PluginQuery, RuntimeInvoker, ServiceQuery};
use serde_json::Value;

use crate::BizError;

/// 将 StepStatus 转换为稳定的字符串表示，避免依赖 Debug 格式
///
/// 统一 cmx-api（HTTP）和 cmx-rpc（gRPC）中重复的 StepStatus 转换逻辑。
pub fn step_status_to_str(status: &StepStatus) -> &'static str {
    match status {
        StepStatus::Success => "Success",
        StepStatus::Failed => "Failed",
        StepStatus::Skipped => "Skipped",
        StepStatus::DebugPaused => "DebugPaused",
    }
}

/// 服务编排执行的核心结果（协议无关）
///
/// 封装 Orchestrator 执行编排的结果，供 cmx-api / cmx-rpc 等协议层
/// 转换为各自的响应格式（HTTP JSON / protobuf）。
#[derive(Debug, Clone)]
pub struct ServiceExecuteCoreResult {
    /// 是否执行成功（所有节点都成功执行则为 true）
    pub success: bool,
    /// 最终输出结果（最后一个节点的输出，失败时为 None）
    pub final_output: Option<Value>,
    /// 各步骤执行记录（按执行顺序，StepStatus 已转为字符串）
    pub steps: Vec<CoreExecutionStep>,
    /// 总执行耗时（微秒，从开始到结束的总时间）
    pub total_elapsed_us: u64,
    /// 结构化错误信息（失败时包含错误摘要）
    pub error: Option<CoreOrchestrationError>,
    /// 是否触发了调试暂停
    pub debug_triggered: Option<bool>,
    /// 调试准备结果（触发调试暂停时包含调试信息）
    pub debug_prepare_result: Option<DebugPrepareResult>,
}

/// 协议无关的执行步骤记录
///
/// 与 `cmx_core::ExecutionStep` 的区别：`status` 字段已转为字符串，
/// 便于协议层直接使用，无需各自重复 StepStatus 匹配逻辑。
#[derive(Debug, Clone)]
pub struct CoreExecutionStep {
    /// 节点ID（对应 Flow JSON 中的 node.id）
    pub node_id: String,
    /// 节点名称（对应 Flow JSON 中的 node.data.name）
    pub node_name: String,
    /// 节点类型（如 skylake-func、skylake-switch）
    pub node_type: String,
    /// 步骤执行状态（"Success" / "Failed" / "Skipped" / "DebugPaused"）
    pub status: String,
    /// 步骤输出（函数执行结果，失败时可能为 None）
    pub output: Option<Value>,
    /// 执行耗时（微秒）
    pub elapsed_us: u64,
    /// 步骤级错误信息（失败时包含具体错误描述）
    pub error: Option<String>,
    /// 上一步的输出（失败时便于排错，记录失败前的数据上下文）
    pub previous_output: Option<Value>,
}

/// 协议无关的编排错误信息
#[derive(Debug, Clone)]
pub struct CoreOrchestrationError {
    /// 错误摘要信息（人类可读的错误描述）
    pub message: String,
}

/// 将 `ExecutionStep` 转换为协议无关的 `CoreExecutionStep`
///
/// 主要工作是将 `StepStatus` 枚举转换为稳定的字符串表示。
fn map_execution_step(step: ExecutionStep) -> CoreExecutionStep {
    CoreExecutionStep {
        node_id: step.node_id,
        node_name: step.node_name,
        node_type: step.node_type,
        status: step_status_to_str(&step.status).to_string(),
        output: step.output,
        elapsed_us: step.elapsed_us,
        error: step.error,
        previous_output: step.previous_output,
    }
}

/// 将 `OrchestrationError` 转换为协议无关的 `CoreOrchestrationError`
fn map_orchestration_error(e: OrchestrationError) -> CoreOrchestrationError {
    CoreOrchestrationError {
        message: e.message,
    }
}

/// 服务编排执行的核心逻辑（协议无关）
///
/// 构造 Orchestrator 并执行服务编排，将 `OrchestrationResult` 转换为
/// 协议无关的 `ServiceExecuteCoreResult`（含 StepStatus 转字符串）。
///
/// # 参数
/// - `runtime`: WASM 运行时调用器
/// - `plugin_query`: 插件查询器
/// - `service_query`: 服务查询器
/// - `default_db_id`: 默认数据库ID（事务框未指定数据库时使用）
/// - `service_key`: 服务唯一标识
/// - `svr_ctx`: 服务调用上下文（包含 initial_input、headers、time_in、request_id）
/// - `options`: 执行选项（控制是否返回 steps 数据、调试行为）
///
/// # 返回值
/// - `Err(BizError)`: 基础设施错误（服务未找到、编排配置缺失、内部错误等）
/// - `Ok(ServiceExecuteCoreResult { success: false, ... })`: 编排执行完成但有节点失败
/// - `Ok(ServiceExecuteCoreResult { success: true, ... })`: 编排执行成功
pub async fn execute_service(
    runtime: &Arc<dyn RuntimeInvoker>,
    plugin_query: &Arc<dyn PluginQuery>,
    service_query: &Arc<dyn ServiceQuery>,
    default_db_id: &str,
    service_key: &str,
    svr_ctx: SVRContext,
    options: ExecuteOptions,
) -> Result<ServiceExecuteCoreResult, BizError> {
    // ==================== 1. 构造 Orchestrator ====================

    let orchestrator = cmx_service::Orchestrator::new(
        runtime.clone(),
        plugin_query.clone(),
        service_query.clone(),
        default_db_id.to_string(),
    );

    // ==================== 2. 执行服务编排 ====================

    let result = orchestrator
        .execute_service(service_key, svr_ctx, options)
        .await
        .map_err(|e| {
            tracing::error!(
                target: "cmx_biz",
                service_key = %service_key,
                error = %e,
                "服务编排执行失败"
            );
            BizError::business(format!("服务执行失败: {}", e))
        })?;

    // ==================== 3. 映射结果（StepStatus 转字符串） ====================

    let core_result = ServiceExecuteCoreResult {
        success: result.success,
        final_output: result.output,
        steps: result.steps.into_iter().map(map_execution_step).collect(),
        total_elapsed_us: result.total_elapsed_us,
        error: result.error.map(map_orchestration_error),
        debug_triggered: result.debug_triggered,
        debug_prepare_result: result.debug_prepare_result,
    };

    Ok(core_result)
}
