//! OpenAPI 文档配置模块
//!
//! 提供 OpenAPI 文档的定义和配置

use utoipa::OpenApi;

/// CMX API OpenAPI 文档
#[derive(OpenApi)]
#[openapi(
    info(
        title = "CMX API",
        version = "1.0.0",
        description = "CMX 系统 API 文档",
    ),
    paths(
        // Module handlers（CRUD + 包）已拆分迁至 cmx-biz-api（ModuleCrudModule）
        // + cmx-plugin-api（ModulePackageModule）。
        // Plugin/TableMetadata/Marketplace handlers 已迁至 cmx-plugin-api（PluginApiDoc）。
        //服务
        crate::handlers::service::handler::service_call,
        crate::handlers::service::handler::execute_service,
        crate::handlers::service::handler::execute_service_by_key,
        // crate::handlers::service::handler::list_services,
        crate::handlers::service::handler::page_services,
        crate::handlers::service::handler::get_service,
        crate::handlers::service::handler::get_services_by_plugin,
        crate::handlers::service::handler::delete_service,
        crate::handlers::service::handler::service_exists,
        crate::handlers::service::handler::get_openapi_spec,

        // Marketplace handlers 已迁至 cmx-plugin-api（PluginApiDoc）。
        // Storage handlers 已迁至 cmx-storage-api（StorageApiDoc）。
        // Auth handlers
        // OAuth2 handlers
        // API Key 管理接口
        // OAuth2 客户端管理接口
        // IAM User handlers
        // IAM Role handlers
        // IAM RoleGroup handlers
        // IAM Permission handlers
        // IAM User temp role handlers（阶段1新增）
        // IAM Rule handlers（阶段2新增）
        // IAM Audit handlers（阶段5新增）
        // Dev handlers 已 feature-gate（dev-tools），不进 Swagger。
        // AI handlers 已迁至 cmx-ai-api（AiApiDoc），由 platform-app OpenApi::merge() 聚合。
    ),

    components(
        schemas(
            // Module schemas 已拆分迁至 cmx-biz-api（Module/ForCreate/ForUpdate）
            // + cmx-plugin-api（ModuleImportResponse）。
            // Plugin schemas 已迁至 cmx-plugin-api（PluginApiDoc）。
            crate::Pagination,
            // cmx-api service models
            crate::handlers::service::models::FunctionCallRequest,
            crate::handlers::service::models::FunctionCallResponse,
            crate::handlers::service::models::ServiceExecuteRequest,
            crate::handlers::service::models::ServiceExecuteResponse,
            crate::handlers::service::models::ServiceExecutionStep,
            crate::handlers::service::models::ServiceDeleteQuery,
            crate::handlers::service::models::ServiceListItem,
            crate::handlers::service::models::ServiceDetailResponse,
            // Marketplace schemas 已迁至 cmx-plugin-api（PluginApiDoc）。
            // Storage schemas 已迁至 cmx-storage-api（StorageApiDoc）。
            // Auth / API Key / OAuth2 / IAM schemas 已迁至 cmx-iam-api（IamApiDoc）。
            // Dev schemas 已 feature-gate（dev-tools），不进 Swagger。
            // AI schemas 已迁至 cmx-ai-api（AiApiDoc）。
        )
    )
)]
pub struct ApiDoc;
