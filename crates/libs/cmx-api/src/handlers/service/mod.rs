//! 服务调用 Handler
//!
//! 提供 WASM 插件服务调用的 HTTP 接口。

pub mod handler;
pub mod models;

pub use handler::{service_call, execute_service, list_services, get_service, get_services_by_plugin, delete_service};

// 重新导出请求/响应结构体，方便外部使用
pub use models::{
    FunctionCallRequest, FunctionCallResponse,
    ServiceExecuteRequest, ServiceExecuteResponse, ServiceExecutionStep,
    ServiceGetQuery, ServiceByPluginQuery, ServiceDeleteQuery,
    ServiceListItem, ServiceDetailResponse,
};

use crate::app_state::CmxAppState;
use crate::routes::traits::ModuleRoutes;
use axum::{routing::{post, get}, Router};

/// 内部路由（不含前缀）
fn inner_routes() -> Router<CmxAppState> {
    Router::new()
        .route("/call", post(service_call))
        .route("/execute", post(execute_service))
        .route("/list", get(list_services))
        .route("/by-plugin", get(get_services_by_plugin))
        .route("/get", get(get_service))
        .route("/delete", post(delete_service))
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
