//! 开发工具 API 模块
//!
//! 提供模板管理等 HTTP API

pub mod handler;
pub mod platform;
pub mod request;
pub mod response;

use axum::Router;
use axum::routing::{delete, get, post};

use crate::app_state::CmxAppState;
use crate::routes::traits::ModuleRoutes;

pub use handler::{create_project, list_templates};

fn inner_routes() -> Router<CmxAppState> {
    Router::new()
        // 列出可用脚手架模板（插件 / 页面 / 服务模板等）
        .route("/templates", get(list_templates))
        // 基于模板生成新项目骨架（落盘到工作区）
        .route("/projects", post(create_project))
        // —— 二次开发平台端点 ——
        // W5：孤儿端点归属 —— 扩展工作区注册
        .route("/vscode/register", post(platform::vscode_register))
        .route("/workspaces", get(platform::list_workspaces))
        // W1：构建作业
        .route(
            "/build/jobs",
            get(platform::list_build_jobs).post(platform::submit_build_job),
        )
        .route("/build/jobs/{id}", get(platform::get_build_job))
        .route("/build/jobs/{id}/logs", get(platform::stream_build_logs))
        // W3：触发绑定
        .route(
            "/trigger/bindings",
            get(platform::list_trigger_bindings).post(platform::save_trigger_binding),
        )
        .route(
            "/trigger/bindings/{id}",
            delete(platform::delete_trigger_binding),
        )
}

pub struct DevModule;

impl ModuleRoutes for DevModule {
    fn routes(self) -> Router<CmxAppState> {
        Router::new().nest("/dev", inner_routes())
    }

    fn prefix() -> &'static str {
        "dev"
    }

    fn module_name(&self) -> &'static str {
        "dev"
    }
}
