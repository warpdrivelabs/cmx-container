//! 调试 API 模块
//!
//! 提供插件调试会话管理等 HTTP API

pub mod handler;
pub mod request;
pub mod response;

use axum::Router;
use axum::routing::{get, post};

use crate::app_state::CmxAppState;
use crate::routes::traits::ModuleRoutes;

pub use handler::{debug_plugin, get_current_debug_session};

fn inner_routes() -> Router<CmxAppState> {
    Router::new()
        .route("/{name}", post(debug_plugin))
        .route("/current", get(get_current_debug_session))
}

pub struct DebugModule;

impl ModuleRoutes for DebugModule {
    fn routes(self) -> Router<CmxAppState> {
        Router::new().nest("/debug", inner_routes())
    }

    fn prefix() -> &'static str {
        "debug"
    }

    fn module_name(&self) -> &'static str {
        "debug"
    }
}
