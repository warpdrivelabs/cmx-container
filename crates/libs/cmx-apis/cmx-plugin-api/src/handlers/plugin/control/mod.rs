/*
//! 插件管控 API 模块
//!
//! 提供集中式管控接口，仅执行 DDL/DML + 文件推送，
//! 不触发本地运行时加载，完成后发布 RuntimeLoad 通知。

pub mod handler;
pub mod request;
pub mod response;

use axum::routing::post;
use axum::Router;

use cmx_api_core::CmxAppState;
use cmx_api_core::ModuleRoutes;

pub use handler::*;

fn inner_routes() -> Router<CmxAppState> {
    Router::new()
        .route("/deploy", post(control_deploy))
        .route("/install", post(control_install))
        .route("/upgrade", post(control_upgrade))
        .route("/downgrade", post(control_downgrade))
        .route("/uninstall", post(control_uninstall))
}

/// 创建管控路由
pub fn control_routes() -> Router<CmxAppState> {
    inner_routes()
}

/// 插件管控模块路由
pub struct PluginControlModule;

impl ModuleRoutes for PluginControlModule {
    fn routes(self) -> Router<CmxAppState> {
        Router::new().nest("/plugin/control", inner_routes())
    }

    fn prefix() -> &'static str {
        "plugin/control"
    }

    fn module_name(&self) -> &'static str {
        "plugin-control"
    }
}
*/