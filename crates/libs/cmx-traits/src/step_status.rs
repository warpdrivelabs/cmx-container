//! StepStatus 字符串编解码（跨模块单一来源）。
//!
//! 提供 `StepStatus`（cmx-core 枚举）与稳定字符串表示之间的双向转换，
//! 统一 cmx-rpc（gRPC）与 cmx-api（HTTP）等协议层的 StepStatus 序列化逻辑。
//!
//! # 设计说明
//!
//! 原实现位于 cmx-biz，但 cmx-rpc（基础设施层）需要复用同一转换逻辑，
//! 为消除 cmx-rpc → cmx-biz 的反向依赖，将此纯函数工具迁移至 cmx-traits 抽象层，
//! cmx-biz 通过 `pub use` 重导出以保持向后兼容。

use cmx_core::StepStatus;

/// 将 [`StepStatus`] 转换为稳定的字符串表示，避免依赖 Debug 格式。
///
/// 与 [`parse_step_status`] 互为逆运算，作为 str↔enum 双向转换的单一来源。
pub fn step_status_to_str(status: &StepStatus) -> &'static str {
    match status {
        StepStatus::Success => "Success",
        StepStatus::Failed => "Failed",
        StepStatus::Skipped => "Skipped",
        StepStatus::DebugPaused => "DebugPaused",
    }
}

/// 将 [`StepStatus`] 字符串表示解析回枚举（[`step_status_to_str`] 的逆运算）。
///
/// 未知字符串降级为 `Failed` 并记录 warn。
///
/// # Arguments
///
/// * `status` - 由 [`step_status_to_str`] 产生的字符串。
pub fn parse_step_status(status: &str) -> StepStatus {
    match status {
        "Success" => StepStatus::Success,
        "Failed" => StepStatus::Failed,
        "Skipped" => StepStatus::Skipped,
        "DebugPaused" => StepStatus::DebugPaused,
        _ => {
            tracing::warn!(
                target: "cmx_traits",
                raw_status = %status,
                "收到未知的 StepStatus 字符串，按 Failed 处理（请升级 cmx-core 或检查版本对齐）"
            );
            StepStatus::Failed
        }
    }
}
