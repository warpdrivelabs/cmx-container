//! cmx-web-monitor —— 通用技术监控（对标 cmx-web-chassis 的通用性）。
//!
//! 任何服务把本 crate 与 chassis 并列组装即得统一「技术监控」：
//! - **请求遥测**：[`observe`] 中间件采集连接/协议/身份/方法/参数/耗时/字节 → 环形缓冲。
//! - **系统指标**：[`spawn_system_sampler`] 后台采 CPU/内存/网络/磁盘/负载。
//! - **DB 池**：读平台 PG 单例各数据源连接池使用。
//! - **页面 + 端点**：[`monitor_routes`] 挂 `/_mon`（页）+ `/_mon/tech-stats`（数据）。
//!
//! 与「业务监控」正交：业务盘各服务自持（如 flow 的流程盘），技术盘一处实现处处可用。
//! 身份读取经 [`set_identity_provider`] 注入解耦（各服务 scope 不同）。

pub mod db;
pub mod handlers;
pub mod identity;
pub mod middleware;
pub mod resp;
pub mod system;
pub mod topology;

pub use handlers::{set_service_name, tech_dashboard, tech_stats};
pub use identity::{Identity, current_identity, set_identity_provider};
pub use middleware::{
    CallRecord, client_stats, observe, requests_snapshot, sse_connect, sse_disconnect,
};
pub use system::{SystemMetrics, spawn_system_sampler, system_snapshot};
pub use topology::{
    ProbeResult, ServiceDep, set_topology_provider, spawn_topology_prober, topology_snapshot,
};

use axum::Router;
use axum::routing::get;

/// 技术监控路由（挂根级，与业务页并列；免认证）。
///
/// 提供 `GET /_mon`（页）+ `GET /_mon/tech-stats`（合并数据）。页面 JS 从自身路径推导数据 URL，
/// 故整组一起 nest 到别处也能工作。
pub fn monitor_routes<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/_mon", get(handlers::tech_dashboard))
        .route("/_mon/tech-stats", get(handlers::tech_stats))
        .route("/_mon/deps", get(handlers::deps_stats))
}
