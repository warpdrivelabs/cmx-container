//! 开发工具 API 模块
//!
//! 提供模板管理等 HTTP API

pub mod handler;
pub mod request;
pub mod response;

use axum::Router;
use axum::routing::{get, post};

use crate::app_state::CmxAppState;
use crate::routes::traits::ModuleRoutes;

pub use handler::{create_project, list_templates};

fn inner_routes() -> Router<CmxAppState> {
    Router::new()
        // 列出可用脚手架模板（插件 / 页面 / 服务模板等）
        .route("/templates", get(list_templates))
        // 基于模板生成新项目骨架（落盘到工作区）
        .route("/projects", post(create_project))
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
