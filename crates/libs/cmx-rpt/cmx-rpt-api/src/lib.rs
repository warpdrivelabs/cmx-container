//! cmx-rpt-api —— 报表模块的**平台适配薄壳**（keep-wired，对标 cmx-flow-api）。
//!
//! 抽核后（对标 flow「一芯多壳」）：全部 handler + 路由表 + 监控大盘已迁至独立 workspace
//! `../cmx-report/` 的平台中立核 `cmx-rpt-app`（`report_routes::<S>()`）。本 crate 只剩装配胶水：
//! `ReportModule` 实现 cmx-api-core 的 `ModuleRoutes`，在平台状态 `CmxAppState` 上实例化那张路由表，
//! 由 web-server 合并（cmx-api-core 不反向依赖本 crate，避免环）。
//!
//! 端点路径与迁移前完全一致（`/report-design/*`、`/report-source-bindings*`、`/rpt/compute`，
//! `/api` 前缀由 web-server nest 加）。独立壳 `cmx-rpt-server` 复用同一核 `report_routes::<()>()`。

use axum::Router;

use cmx_api_core::CmxAppState;
use cmx_api_core::routes::traits::ModuleRoutes;

mod proxy;
pub use proxy::{ReportProxyModule, with_report_page_proxy};

/// 报表模块路由聚合（实现 cmx-api-core 的 ModuleRoutes，由 web-server 合并进主路由）。
pub struct ReportModule;

impl ModuleRoutes for ReportModule {
    fn routes(self) -> Router<CmxAppState> {
        // 平台中立核在平台状态类型上实例化：与独立壳 report_routes::<()>() 零 handler 漂移。
        cmx_rpt_app::report_routes::<CmxAppState>()
    }

    fn prefix() -> &'static str {
        "report"
    }

    fn module_name(&self) -> &'static str {
        "report"
    }
}
