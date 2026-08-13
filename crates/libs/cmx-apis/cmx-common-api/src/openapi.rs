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

/// 门户业务 OpenAPI 文档切片（`/api` 下门户 / 设计器接口，统一 tag「门户接口」）。
///
/// 不带 `info`（切片惯例），由 cmx-platform-app `merged_openapi()` 合并进主文档；
/// 独立门户微服务（cmx-portal-server）复用同一装配核，Swagger 同样可见。
#[derive(OpenApi)]
#[openapi(
    paths(
        // AI 对话中继 + 本地编辑代理
        crate::handlers::portal::ai::ai_chat,
        crate::handlers::portal::ai::agent_capabilities,
        crate::handlers::portal::ai::agent_message,
        crate::handlers::portal::ai::agent_message_stream,
        crate::handlers::portal::ai::agent_approval,
        // 工作区节点
        crate::handlers::portal::meta::list_workspace_nodes,
        crate::handlers::portal::meta::get_workspace_node,
        crate::handlers::portal::meta::save_workspace_node,
        crate::handlers::portal::meta::delete_workspace_node,
        // 表单页 / 原生页面 / HTML 页面
        crate::handlers::portal::pages::list_form_pages,
        crate::handlers::portal::pages::save_form_page,
        crate::handlers::portal::pages::get_form_page,
        crate::handlers::portal::pages::list_native_pages,
        crate::handlers::portal::pages::save_native_page,
        crate::handlers::portal::pages::batch_native_pages,
        crate::handlers::portal::pages::get_native_page,
        crate::handlers::portal::pages::list_html_pages,
        crate::handlers::portal::pages::save_html_page,
        crate::handlers::portal::pages::batch_html_pages,
        crate::handlers::portal::pages::get_html_page,
        // 事实数据 / 帮助中心
        crate::handlers::portal::data::list_facts,
        crate::handlers::portal::data::get_fact_post,
        crate::handlers::portal::data::get_fact_path,
        crate::handlers::portal::data::help_catalog,
        crate::handlers::portal::data::help_get_post,
        crate::handlers::portal::data::help_get_path,
        crate::handlers::portal::data::help_save_doc,
        crate::handlers::portal::data::help_delete_doc,
        // 通知中心（任务/消息/日志 + SSE 主动推送）
        crate::handlers::portal::notify::notify_list,
        crate::handlers::portal::notify::notify_centers,
        crate::handlers::portal::notify::notify_counts,
        crate::handlers::portal::notify::notify_publish,
        crate::handlers::portal::notify::notify_mark_read,
        crate::handlers::portal::notify::notify_stream,
        // 功能启动器
        crate::handlers::portal::launcher::launcher_resolve,
        // 注册表只读派生（DAM）+ 服务目录 + 模块清单与资源
        crate::handlers::portal::registry::registry_domains,
        crate::handlers::portal::registry::registry_apps,
        crate::handlers::portal::registry::registry_modules,
        crate::handlers::portal::registry::registry_dam,
        crate::handlers::portal::registry::service_catalog_list,
        crate::handlers::portal::registry::service_catalog_get,
        crate::handlers::portal::registry::list_modules,
        crate::handlers::portal::registry::get_module_manifest,
        crate::handlers::portal::registry::get_module_resource,
        crate::handlers::portal::registry::module_resources,
    ),
    components(
        schemas(crate::handlers::portal::notify::NotifyMarkInput)
    )
)]
pub struct PortalApiDoc;
