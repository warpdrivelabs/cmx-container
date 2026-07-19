//! cmx-rpt-api —— 报表模块的 HTTP 层。
//!
//! 薄 axum handler：提取参数 → 调 `cmx_rpt_store_pg` 服务函数 → `ApiResp` 信封。
//! `ReportModule` 实现 cmx-api 的 `ModuleRoutes`，聚合报表设计工作台 13 条路由 + `/rpt/compute`。
//! 由 web-server（而非 cmx-api）合并 `ReportModule.routes()`，故 cmx-api 不反向依赖本 crate（无环）。
//! 端点路径与迁移前完全一致（`/report-design/*`、`/rpt/compute`，`/api` 前缀由 web-server nest 加）。

pub mod handlers;

use axum::Router;
use axum::routing::{get, post};

use cmx_api::CmxAppState;
use cmx_api::routes::traits::ModuleRoutes;

/// 报表模块路由聚合（实现 cmx-api 的 ModuleRoutes，由 web-server 合并进主路由）。
pub struct ReportModule;

impl ModuleRoutes for ReportModule {
    fn routes(self) -> Router<CmxAppState> {
        Router::new()
            // 报表设计工作台：全部数据来自 fico-db 物理表。
            .route(
                "/report-design/overview",
                get(handlers::report_design_overview),
            )
            .route(
                "/report-design/reports",
                get(handlers::report_design_reports).post(handlers::report_design_create_report),
            )
            .route(
                "/report-design/elements",
                get(handlers::report_design_elements),
            )
            // 报表应用工作台：会计日历（期间）+ 合并组织架构（组织树）读服务，来自 fico-db。
            .route(
                "/report-design/calendar",
                get(handlers::report_design_calendar),
            )
            .route(
                "/report-design/consol-org",
                get(handlers::report_design_consol_org),
            )
            .route(
                "/report-design/reports/{code}",
                get(handlers::report_design_report_detail)
                    .delete(handlers::report_design_delete_report),
            )
            .route(
                "/report-design/reports/{code}/versions",
                post(handlers::report_design_create_version),
            )
            .route(
                "/report-design/reports/{code}/versions/{version}/default",
                post(handlers::report_design_set_default_version),
            )
            // 报表两模式加载存储（ReportModel 单一事实源）：
            //   layout = 设计版式（cr_report_fmt BLOB + 关系投影 sheet/region/row/col）
            //   data   = 报表数据（cr_cell_data 按 org+period，读走 ZmcDataSet 零拷贝）
            .route(
                "/report-design/reports/{code}/layout",
                get(handlers::report_design_load_layout)
                    .post(handlers::report_design_save_layout),
            )
            .route(
                "/report-design/reports/{code}/data/query",
                post(handlers::report_design_query_data),
            )
            .route(
                "/report-design/reports/{code}/data",
                post(handlers::report_design_save_data),
            )
            // 计算态：真算（装载公式→递归求值→落 cr_cell_data）+ 函数目录（设计器向导）。
            .route(
                "/report-design/reports/{code}/compute",
                post(handlers::report_design_compute),
            )
            .route(
                "/report-design/functions",
                get(handlers::report_design_functions),
            )
            // 旧 html 报表设计器预览兼容接口；新工作台不使用。
            .route("/rpt/compute", post(handlers::rpt_compute))
    }

    fn prefix() -> &'static str {
        "report"
    }

    fn module_name(&self) -> &'static str {
        "report"
    }
}
