//! 路由配置模块
//!
//! 负责配置应用程序的所有 HTTP 路由，包括 API 路由和 Swagger 文档路由。

use axum::Router;
use cmx_api::CmxAppState;
use cmx_api::routes::routes_impl::{api_routes, swagger_routes};

/// 配置所有 API 路由
///
/// 直接调用 cmx-api 的统一路由注册，返回配置好的 Axum Router。
///
/// # Returns
///
/// 配置完成的 Axum Router 实例，已挂载所有 API 端点。
pub fn routes() -> Router<CmxAppState> {
    api_routes()
}

/// 获取 Swagger 文档路由
///
/// 返回 Swagger UI 和 OpenAPI 规范的路由。
///
/// # Returns
///
/// Axum Router 实例，包含 Swagger 文档相关端点。
pub fn get_swagger_routes() -> Router {
    swagger_routes()
}
