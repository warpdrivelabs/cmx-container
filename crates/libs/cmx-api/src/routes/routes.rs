//! 集中路由注册模块
//!
//! 提供统一的路由注册入口，简化 web-server 的路由配置

use crate::app_state::CmxAppState;
use crate::handlers::application;
use crate::handlers::debug;
use crate::handlers::dev;
use crate::handlers::domain;
use crate::handlers::module;
use crate::handlers::plugin;
use crate::handlers::service;
use crate::handlers::sys_datasource;
use crate::handlers::table_metadata;
use crate::openapi::ApiDoc;
use crate::routes::traits::ModuleRoutes;
use axum::Router;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

/// 将模块路由注册到 /api 前缀下
fn with_api_prefix(router: Router<CmxAppState>) -> Router<CmxAppState> {
    Router::new().nest("/api", router)
}

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

    // 注册 Domain 模块路由（使用 ModuleRoutes）
    let router = router.merge(domain::DomainModule.routes());

    // 注册 Application 模块路由（使用 ModuleRoutes）
    let router = router.merge(application::ApplicationModule.routes());

    // 注册 Module 模块路由（使用 ModuleRoutes）
    let router = router.merge(module::ModuleHandler.routes());

    // 注册 SysDatasource 模块路由（使用 ModuleRoutes）
    let router = router.merge(sys_datasource::SysDatasourceModule.routes());

    // 注册插件管理路由（使用 ModuleRoutes）
    let router = router.merge(plugin::PluginModule.routes());

    // 注册表元数据查询路由（使用 ModuleRoutes）
    let router = router.merge(table_metadata::TableMetadataModule.routes());

    // 注册服务调用路由（使用 ModuleRoutes）
    let router = router.merge(service::ServiceModule.routes());

    // 注册调试路由（使用 ModuleRoutes）
    let router = router.merge(debug::DebugModule.routes());

    // 注册开发工具路由（使用 ModuleRoutes）
    

    router.merge(dev::DevModule.routes())
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
