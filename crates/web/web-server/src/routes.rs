//! 路由配置模块
//!
//! 简单调用 cmx-api 的统一路由注册

use axum::Router;
use cmx_api::routes::api_routes;
use cmx_api::CmxAppState;

/// 配置所有 API 路由
///
/// 直接调用 cmx-api 的统一路由注册
pub fn routes() -> Router<CmxAppState> {
    api_routes()
}
