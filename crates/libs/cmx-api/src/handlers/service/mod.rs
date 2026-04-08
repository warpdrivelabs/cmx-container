//! 服务调用 Handler
//!
//! 提供 WASM 插件服务调用的 HTTP 接口。

pub mod handler;

pub use handler::{service_call, execute_orchestration};

use crate::app_state::CmxAppState;
use crate::routes::traits::ModuleRoutes;
use axum::routing::post;
use axum::Router;

/// 内部路由（不含前缀）
fn inner_routes() -> Router<CmxAppState> {
    Router::new()
        .route("/call", post(service_call))
        .route("/orchestration", post(execute_orchestration))
}

/// Service 模块路由
pub struct ServiceModule;

impl ModuleRoutes for ServiceModule {
    fn routes(self) -> Router<CmxAppState> {
        Router::new().nest("/service", inner_routes())
    }

    fn prefix() -> &'static str {
        "service"
    }

    fn module_name(&self) -> &'static str {
        "service"
    }
}
