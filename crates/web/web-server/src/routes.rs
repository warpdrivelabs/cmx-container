//! 路由配置模块
//!
//! 简单调用 cmx-api 的统一路由注册

use axum::Router;
use cmx_api::CmxAppState;
use cmx_api::route::routes::api_routes;

/// 配置所有 API 路由
///
/// 直接调用 cmx-api 的统一路由注册
pub fn routes() -> Router<CmxAppState> {
    api_routes()
}
