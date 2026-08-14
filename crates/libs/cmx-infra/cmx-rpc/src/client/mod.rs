//! gRPC 客户端共享设施。
//!
//! - [`auth_outbound`]：出站鉴权 header 注入（`apply_auth_metadata`）。
//! - [`infra`]：共享基础设施（[`GrpcInfrastructure`]，服务发现缓存/超时/重试配置）。
//! - [`retry`]：重试循环（[`with_retry`]，指数退避 + 总预算）。
//!
//! 具体领域的 gRPC 客户端（orchestrator / resource_data 等）已迁至 `cmx-rpcs/*`
//! 皮肤 crate（如 `cmx-orchestrator-rpc`），经 [`GrpcInfrastructure`] 复用本模块设施。

pub mod auth_outbound;
pub mod infra;
pub mod retry;

pub use infra::GrpcInfrastructure;

/// 安全解析 JSON 字符串，解析失败时记录 warn 日志并降级为 [`serde_json::Value::Null`]。
pub fn safe_parse_json(raw: &str, context: &str) -> serde_json::Value {
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
