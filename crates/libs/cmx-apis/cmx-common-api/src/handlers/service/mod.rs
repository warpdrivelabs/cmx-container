//! 服务调用 Handler
//!
//! 提供 WASM 插件服务调用的 HTTP 接口。

pub mod handler;
pub mod models;

pub use handler::{
    delete_service, execute_service, execute_service_by_key, get_service, get_services_by_plugin,
    page_services, service_call, service_exists,
};

// 重新导出请求/响应结构体，方便外部使用
pub use models::{
    FunctionCallRequest, FunctionCallResponse, ServiceByPluginQuery, ServiceDeleteQuery,
    ServiceDetailResponse, ServiceExecuteRequest, ServiceExecuteResponse, ServiceExecutionStep,
    ServiceGetQuery, ServiceListItem,
};

use crate::app_state::CmxAppState;
use crate::routes::traits::ModuleRoutes;
use axum::{
    Router,
    routing::{get, post},
};

use handler::get_openapi_spec;

/// 内部路由（不含前缀）
///
/// 所有路由统一挂在 `/api/service` 下，提供 WASM 插件服务的调用与元数据管理。
fn inner_routes() -> Router<CmxAppState> {
    Router::new()
        // 调用服务（按 ServiceCallRequest 体内指定 service_key + 参数）
        .route("/call", post(service_call))
        // 执行服务（携带完整执行上下文，多步编排）
        .route("/execute", post(execute_service))
        // 按 service_key 直接执行（路径参数版，便于外部系统直连）
        .route("/execute/{service_key}", post(execute_service_by_key))
        // .route("/list", get(list_services))  // 已废弃，由 /page 取代
        // 分页查询服务定义列表
        .route("/page", post(page_services))
        // 按插件查询其下注册的服务清单
        .route("/by-plugin", get(get_services_by_plugin))
        // 查询单个服务定义详情
        .route("/get", get(get_service))
        // 删除服务定义
        .route("/delete", post(delete_service))
        // 判断指定 service_key 是否已注册
        .route("/exists", get(service_exists))
        // 导出本平台服务聚合后的 OpenAPI 规范（供外部 SDK 生成）
        .route("/openapi", get(get_openapi_spec))
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
