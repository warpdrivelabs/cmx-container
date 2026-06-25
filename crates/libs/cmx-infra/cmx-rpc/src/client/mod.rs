//! gRPC 领域客户端模块。
//!
//! 按领域拆分为：
//! - [`orchestrator`]：服务编排（`call_service` / `call_function`）。
//! - [`plugin_data`]：插件数据管理（`import_plugin_data` / `cleanup_plugin_data`）。
//!
//! 共享基础设施见 [`infra::GrpcInfrastructure`]，重试逻辑见 [`retry::with_retry`]。

pub mod infra;
pub mod orchestrator;
pub mod plugin_data;
pub mod retry;

pub use orchestrator::orchestrator_client;
pub use plugin_data::plugin_data_client;

/// 安全解析 JSON 字符串，解析失败时记录 warn 日志并降级为 [`serde_json::Value::Null`]。
pub(crate) fn safe_parse_json(raw: &str, context: &str) -> serde_json::Value {
    serde_json::from_str(raw).unwrap_or_else(|e| {
        tracing::warn!(
            target: "cmx_rpc",
            error = %e,
            raw = %raw,
            context = context,
            "RPC 返回 JSON 解析失败，降级为 Null"
        );
        serde_json::Value::Null
    })
}
