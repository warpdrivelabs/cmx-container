//! 集中路由注册模块
//!
//! 提供统一的路由注册入口，简化 web-server 的路由配置

use crate::app_state::CmxAppState;
use crate::handlers::debug;
#[cfg(feature = "dev-tools")]
use crate::handlers::dev;
use crate::handlers::portal;
use crate::handlers::service;
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

    // 认证（AuthModule）+ IAM（IamModule）路由已迁至 cmx-iam-api，由 cmx-platform-app 合并。

    // Domain/Application/Menu/SysDatasource/Form 路由已迁至 cmx-biz-api，由 platform-app 合并。

    // Module 路由（CRUD + 包）已拆分迁至 cmx-biz-api（ModuleCrudModule）
    // + cmx-plugin-api（ModulePackageModule），由 cmx-platform-app 合并。

    // 插件管理 / 表元数据 / 插件市场 路由已迁至 cmx-plugin-api，由 cmx-platform-app 合并。

    // 注册服务调用路由（使用 ModuleRoutes）
    let router = router.merge(service::ServiceModule.routes());

    // 注册调试路由（使用 ModuleRoutes）
    let router = router.merge(debug::DebugModule.routes());

    // 文件存储路由（StorageModule）已迁至 cmx-storage-api，由 cmx-platform-app 合并。

    // 注册门户/设计器业务路由（迁移自 Node 后端，使用 ModuleRoutes）
    let router = router.merge(portal::PortalModule.routes());

    // AI 中继路由（AiModule）已迁至 cmx-ai-api，由 cmx-platform-app 合并。

    // 开发工具路由（仅 dev-tools feature 启用时注册；违反集群无状态约束，生产禁用）
    #[cfg(feature = "dev-tools")]
    let router = {
        tracing::warn!("dev-tools feature 已启用：开发脚手架端点暴露，仅限单节点 dev，不可水平扩展！");
        router.merge(dev::DevModule.routes())
    };

    // 注册健康检查路由（无需认证，供 Docker HEALTHCHECK 和负载均衡器使用）
    let router = router.route("/health", get(health_check));

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
