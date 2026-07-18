//! 集中路由注册模块
//!
//! 提供统一的路由注册入口，简化 web-server 的路由配置

use crate::app_state::CmxAppState;
use crate::handlers::ai;
use crate::handlers::application;
use crate::handlers::auth;
use crate::handlers::debug;
use crate::handlers::dev;
use crate::handlers::domain;
use crate::handlers::form;
use crate::handlers::iam;
use crate::handlers::marketplace;
use crate::handlers::menu;
use crate::handlers::module;
use crate::handlers::plugin;
use crate::handlers::portal;
use crate::handlers::service;
use crate::handlers::storage;
use crate::handlers::sys_datasource;
use crate::handlers::table_metadata;
use crate::openapi::ApiDoc;
use crate::routes::traits::ModuleRoutes;
use axum::{Json, Router, routing::get};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

/// 注册所有 API 路由
///
/// # 参数
/// * 无
///
/// # 返回值
/// 返回配置好的 Axum 路由器
///
/// # 示例
/// ```rust
/// use cmx_api::routes::api_routes;
/// use cmx_api::CmxAppState;
///
/// let router = api_routes().with_state(CmxAppState::default());
/// ```
///
/// # 注册的路由
pub fn api_routes() -> Router<CmxAppState> {
    let router = Router::new();

    // 注册认证模块路由（使用 ModuleRoutes）
    let router = router.merge(auth::AuthModule.routes());

    // 注册 Domain 模块路由（使用 ModuleRoutes）
    let router = router.merge(domain::DomainModule.routes());

    // 注册 IAM 模块路由（使用 ModuleRoutes）
    let router = router.merge(iam::IamModule.routes());

    // 注册 Application 模块路由（使用 ModuleRoutes）
    let router = router.merge(application::ApplicationModule.routes());

    // 注册 Module 模块路由（使用 ModuleRoutes）
    let router = router.merge(module::ModuleHandler.routes());

    // 注册 SysDatasource 模块路由（使用 ModuleRoutes）
    let router = router.merge(sys_datasource::SysDatasourceModule.routes());

    // 注册 Form 模块路由（使用 ModuleRoutes）
    let router = router.merge(form::FormModule.routes());

    // 注册 Menu 模块路由（使用 ModuleRoutes）
    let router = router.merge(menu::MenuModule.routes());

    // 注册插件管理路由（使用 ModuleRoutes）
    let router = router.merge(plugin::PluginModule.routes());

    // 注册插件管控路由（使用 ModuleRoutes）
    // let router = router.merge(plugin::control::PluginControlModule.routes());

    // 注册表元数据查询路由（使用 ModuleRoutes）
    let router = router.merge(table_metadata::TableMetadataModule.routes());

    // 注册服务调用路由（使用 ModuleRoutes）
    let router = router.merge(service::ServiceModule.routes());

    // 注册调试路由（使用 ModuleRoutes）
    let router = router.merge(debug::DebugModule.routes());

    // 注册插件市场路由（使用 ModuleRoutes）
    let router = router.merge(marketplace::MarketplaceModule.routes());

    // 注册文件存储路由（使用 ModuleRoutes）
    let router = router.merge(storage::StorageModule.routes());

    // 注册门户/设计器业务路由（迁移自 Node 后端，使用 ModuleRoutes）
    let router = router.merge(portal::PortalModule.routes());

    // 注册 AI 生成中继路由（薄代理转发 OpenCode + SSE，使用 ModuleRoutes）
    let router = router.merge(ai::AiModule.routes());

    // 注册开发工具路由（使用 ModuleRoutes）

    let mut router = router.merge(dev::DevModule.routes());

    // 注册健康检查路由（无需认证，供 Docker HEALTHCHECK 和负载均衡器使用）
    router = router.route("/health", get(health_check));

    router
    // 统一添加 /api 前缀
    // with_api_prefix(router)
}

/// 注册带有 Swagger UI 的 API 路由
///
/// # 参数
/// * 无
///
/// # 返回值
/// 返回配置好的 Axum 路由器，包含 Swagger UI
///
/// # Swagger UI 访问
/// - Swagger UI: http://localhost:port/swagger-ui/
/// - OpenAPI JSON: http://localhost:port/api-docs/openapi.json
pub fn swagger_routes() -> Router {
    Router::new()
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
}

/// 健康检查处理器
///
/// 返回服务运行状态，供 Docker HEALTHCHECK 和负载均衡器探测使用
pub async fn health_check() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok"
    }))
}
