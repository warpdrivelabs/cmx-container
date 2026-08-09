//! 技术监控端点 + 页面。
//!
//! `GET /_mon/tech-stats` → 合并 JSON `{requests, system, db}`（前端每几秒轮询）。
//! `GET /_mon` → 自包含技术监控页 HTML（业务无关，任何服务通用）。

use axum::Json;
use axum::response::Html;
use serde_json::{Value, json};

use crate::resp::ApiResp;

/// 合并技术监控数据：请求遥测 + 系统指标 + DB 池状态 + 服务依赖拓扑 + 本服务标识。
pub async fn tech_stats() -> Json<ApiResp<Value>> {
    let data = json!({
        "service": { "name": SERVICE_NAME.get().map(String::as_str).unwrap_or("cmx") },
        "requests": crate::middleware::requests_snapshot(),
        "system": crate::system::system_snapshot(),
        "db": crate::db::pool_snapshot().await,
        "deps": crate::topology::topology_snapshot().await,
    });
    Json(ApiResp::ok(data))
}

/// 仅服务依赖拓扑（`GET /_mon/deps`）——比全量 tech-stats 轻，供门户集成状态页单独轮询。
pub async fn deps_stats() -> Json<ApiResp<Value>> {
    Json(ApiResp::ok(crate::topology::topology_snapshot().await))
}

/// 技术监控页 HTML（编译期内嵌；服务名占位符 `__SVC_TITLE__` 运行时替换）。
const TECH_HTML: &str = include_str!("../assets/tech-dashboard.html");

/// 服务名（页标题用；服务启动时经 [`set_service_name`] 设定）。
static SERVICE_NAME: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// 设定本服务名（页标题显示，如 "cmx-flow 流程引擎"）。
pub fn set_service_name(name: impl Into<String>) {
    let _ = SERVICE_NAME.set(name.into());
}

/// 技术监控页。
pub async fn tech_dashboard() -> Html<String> {
    let svc = SERVICE_NAME.get().map(String::as_str).unwrap_or("cmx");
    Html(TECH_HTML.replace("__SVC_TITLE__", svc))
}
