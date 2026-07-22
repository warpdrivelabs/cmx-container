//! cmx-job-api —— 异步任务中心的 HTTP 层。
//!
//! 薄 axum handler：提取参数 → 调 [`cmx_job_core::JobManager`] → `ApiResp` 信封 / SSE 流。
//! [`JobModule`] 实现 cmx-api 的 `ModuleRoutes`，聚合任务中心 9 条路由（含 1 条 SSE）。
//! 由 web-server（而非 cmx-api）合并 `JobModule.routes()`，故 cmx-api 不反向依赖本 crate（无环）。
//! 端点路径：`/jobs`、`/jobs/{id}`、`/jobs/{id}/events`(SSE)、`/jobs/{id}/{pause|resume|cancel|restart}`
//! （`/api` 前缀由 web-server nest 加）。

pub mod handlers;

use axum::Router;
use axum::routing::{get, post};

use cmx_api::CmxAppState;
use cmx_api::routes::traits::ModuleRoutes;

/// 任务中心模块路由聚合（实现 cmx-api 的 ModuleRoutes，由 web-server 合并进主路由）。
pub struct JobModule;

impl ModuleRoutes for JobModule {
    fn routes(self) -> Router<CmxAppState> {
        Router::new()
            // 提交（POST）+ 列表（GET）。
            .route(
                "/jobs",
                post(handlers::submit_job).get(handlers::list_jobs),
            )
            // 汇总 SSE 流（列表页实时刷新）——须在 /jobs/{id} 之前声明避免歧义。
            .route("/jobs/events", get(handlers::subscribe_summary))
            // 历史作业查询（RU/HI 分离：归档到 cmx_job_hi）——静态段，须在 /jobs/{id} 之前。
            .route("/jobs/history", get(handlers::list_history))
            .route("/jobs/history/{id}", get(handlers::get_history))
            // 详情（GET）+ 归档（DELETE，仅终态，转移到历史表）。
            .route(
                "/jobs/{id}",
                get(handlers::get_job).delete(handlers::delete_job),
            )
            // SSE 实时进度流。
            .route("/jobs/{id}/events", get(handlers::subscribe_events))
            // 控制：暂停 / 恢复 / 停止 / 重启。
            .route("/jobs/{id}/pause", post(handlers::pause_job))
            .route("/jobs/{id}/resume", post(handlers::resume_job))
            .route("/jobs/{id}/cancel", post(handlers::cancel_job))
            .route("/jobs/{id}/restart", post(handlers::restart_job))
    }

    fn prefix() -> &'static str {
        "job"
    }

    fn module_name(&self) -> &'static str {
        "job"
    }
}
