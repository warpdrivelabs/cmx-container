//! cmx-flow-api —— 流程引擎的 HTTP 层。
//!
//! 薄 axum handler（handlers.rs）+ 视图组装（views.rs）+ 引擎单例（engine.rs）。
//! `FlowModule` 实现 cmx-api 的 `ModuleRoutes`，聚合流程设计态 + 运行态 20 条路由。
//! 由 web-server（而非 cmx-api）合并 `FlowModule.routes()`，故 cmx-api 不反向依赖本 crate（无环）。
//! 端点前缀 `/flow/*`（`/api` 前缀由 web-server nest 加），避开 /api/definitions、/api/users 等既有命名。

pub mod engine;
pub mod handlers;
pub mod views;

use axum::routing::{get, post};
use axum::Router;

use cmx_api::routes::traits::ModuleRoutes;
use cmx_api::CmxAppState;

pub use engine::{flow, spawn_timer_poller, FlowRuntime};

/// 流程模块路由聚合（实现 cmx-api 的 ModuleRoutes，由 web-server 合并进主路由）。
pub struct FlowModule;

impl ModuleRoutes for FlowModule {
    fn routes(self) -> Router<CmxAppState> {
        Router::new()
            // —— 定义（设计器：草稿/发布/装载） ——
            .route("/flow/definitions", get(handlers::get_definitions))
            .route("/flow/design/definitions", get(handlers::list_design_definitions))
            .route("/flow/definitions/draft", post(handlers::save_definition_draft))
            .route("/flow/definitions/{key}", get(handlers::get_definition_detail))
            .route("/flow/definitions/{key}/publish", post(handlers::publish_definition))
            // —— 版本管理（对标报表版本：列表/激活/删除） ——
            .route("/flow/definitions/{key}/versions", get(handlers::list_definition_versions))
            .route(
                "/flow/definitions/{key}/versions/{version}/activate",
                post(handlers::activate_definition_version),
            )
            .route(
                "/flow/definitions/{key}/versions/{version}",
                axum::routing::delete(handlers::delete_definition_version),
            )
            // —— 实例 ——
            .route(
                "/flow/instances",
                get(handlers::list_instances).post(handlers::start_instance),
            )
            .route("/flow/instances/{id}", get(handlers::get_instance))
            .route("/flow/instances/{id}/children", get(handlers::get_children))
            .route("/flow/instances/{id}/cancel", post(handlers::cancel_instance))
            // —— 任务 ——
            .route("/flow/tasks/{id}/complete", post(handlers::complete_task))
            .route("/flow/tasks/{id}/claim", post(handlers::claim_task))
            .route("/flow/tasks/{id}/transfer", post(handlers::transfer_task))
            .route("/flow/tasks/{id}/delegate", post(handlers::delegate_task))
            .route("/flow/tasks/{id}/addsign", post(handlers::add_sign_task))
            // —— 抄送 / 定时器 / 用户 ——
            .route("/flow/users", get(handlers::list_users))
            .route("/flow/cc", get(handlers::list_cc))
            .route("/flow/cc/{id}/read", post(handlers::mark_cc_read))
            .route("/flow/timers/trigger", post(handlers::trigger_timers))
            // —— 子流程组织路由（绑定管理 + 组织树） ——
            .route("/flow/orgs", get(handlers::list_orgs))
            .route(
                "/flow/subflow-bindings",
                post(handlers::upsert_subflow_binding),
            )
            .route(
                "/flow/subflow-bindings/{key}",
                get(handlers::list_subflow_bindings),
            )
            .route(
                "/flow/subflow-bindings/id/{id}",
                axum::routing::delete(handlers::delete_subflow_binding),
            )
    }

    fn prefix() -> &'static str {
        "flow"
    }

    fn module_name(&self) -> &'static str {
        "flow"
    }
}
