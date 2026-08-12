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
            // Auth schemas
            // API Key 管理 schemas
            // OAuth2 客户端管理 schemas
            // OAuth2 schemas
            // IAM schemas
            cmx_core::model::iam::User,
            cmx_core::model::iam::Role,
            cmx_core::model::iam::RoleGroup,
            cmx_core::model::iam::RoleGroupTreeNode,
            cmx_core::model::iam::Permission,
            cmx_core::model::iam::PermissionTreeNode,
            cmx_iam::user::UserForCreate,
            cmx_iam::user::UserForUpdate,
            cmx_iam::user::AssignRolesRequest,
            cmx_iam::role::RoleForCreate,
            cmx_iam::role::RoleForUpdate,
            cmx_iam::role::AssignPermissionsRequest,
            cmx_iam::role::AssignRoleUsersRequest,
            cmx_iam::role::RoleUserSummary,
            cmx_iam::role_group::RoleGroupForCreate,
            cmx_iam::role_group::RoleGroupForUpdate,
            cmx_iam::permission::PermissionForCreate,
            cmx_iam::permission::PermissionForUpdate,
            cmx_iam::permission::BlockedRoleInfo,
            cmx_iam::permission::BlockedPermissionInfo,
            cmx_iam::permission::DeletePermissionOutcome,
            cmx_iam::permission::DeletePermissionResult,
            cmx_iam::permission::DeletePermissionBlocked,
            crate::ApiResp<cmx_core::model::iam::User>,
            crate::ApiResp<cmx_core::model::iam::Role>,
            crate::ApiResp<cmx_core::model::iam::RoleGroup>,
            crate::ApiResp<Vec<cmx_core::model::iam::RoleGroupTreeNode>>,
            crate::ApiResp<cmx_core::model::iam::Permission>,
            crate::ApiResp<Vec<cmx_core::model::iam::PermissionTreeNode>>,
            crate::ApiResp<Vec<cmx_core::model::iam::Role>>,
            crate::ApiResp<Vec<cmx_core::model::iam::RoleGroup>>,
            crate::ApiResp<Vec<cmx_core::model::iam::Permission>>,
            // IAM temp role schemas（阶段1新增）
            cmx_iam::service_traits::UserRoleAssignment,
            crate::ApiResp<cmx_iam::service_traits::UserRoleAssignment>,
            crate::ApiResp<Vec<cmx_iam::service_traits::UserRoleAssignment>>,
            // IAM rule schemas（阶段2新增）
            cmx_iam::rule::entity::ExclusionRule,
            cmx_iam::rule::entity::UpdateExclusionRuleRequest,
            cmx_iam::rule::entity::CreateExclusionRuleRequest,
            cmx_iam::rule::entity::ExclusionRuleItem,
            cmx_iam::rule::entity::ValidateRuleRequest,
            cmx_iam::rule::entity::ValidateRuleResponse,
            crate::ApiResp<cmx_iam::rule::entity::ExclusionRule>,
            crate::ApiResp<Vec<cmx_iam::rule::entity::ExclusionRule>>,
            crate::ApiResp<cmx_iam::rule::entity::ValidateRuleResponse>,
            // IAM audit schemas（阶段5新增）
            cmx_iam::service_traits::EffectivePermissionsResponse,
            cmx_iam::service_traits::RoleSummary,
            cmx_iam::service_traits::PermissionSummary,
            cmx_iam::service_traits::PermissionDiffResponse,
            cmx_iam::service_traits::PermissionUsageStat,
            crate::ApiResp<cmx_iam::service_traits::EffectivePermissionsResponse>,
            crate::ApiResp<cmx_iam::service_traits::PermissionDiffResponse>,
            crate::ApiResp<Vec<cmx_iam::service_traits::PermissionUsageStat>>,
            // Dev schemas 已 feature-gate（dev-tools），不进 Swagger。
            // AI schemas 已迁至 cmx-ai-api（AiApiDoc）。
        )
    )
)]
pub struct ApiDoc;
