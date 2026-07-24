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
                get(handlers::report_design_load_layout).post(handlers::report_design_save_layout),
            )
            .route(
                "/report-design/reports/{code}/data/query",
                post(handlers::report_design_query_data),
            )
            // 打开报表：一次调用取全集（版式+cellMap+元素+函数[+数据]），替代前端顺序多调。
            .route(
                "/report-design/reports/{code}/open",
                post(handlers::report_design_open_report),
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
            // 协同编辑 B 档：语义操作提交（POST）+ 增量拉取追平（GET ?version=&since=）。
            .route(
                "/report-design/reports/{code}/ops",
                get(handlers::report_design_list_ops).post(handlers::report_design_apply_ops),
            )
            // 计算路由设置：报表取数路由绑定（cr_report_source_binding）注册 CRUD。
            //   单据类型 + 组织 → 物理目标(db_id|service)。list 用 {key}，delete 用独立
            //   /id/{id} 子路径避免与 {key} 路由歧义（照 cmx-flow 子流程绑定端点）。
            .route(
                "/report-source-bindings",
                post(handlers::upsert_report_source_binding),
            )
            .route(
                "/report-source-bindings/{key}",
                get(handlers::list_report_source_bindings),
            )
            .route(
                "/report-source-bindings/id/{id}",
                axum::routing::delete(handlers::delete_report_source_binding),
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
