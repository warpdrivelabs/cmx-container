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
        // Domain CRUD handlers
        crate::routes::crud_handlers::domain_crud::create,
        crate::routes::crud_handlers::domain_crud::create_many,
        crate::routes::crud_handlers::domain_crud::get,
        crate::routes::crud_handlers::domain_crud::update,
        crate::routes::crud_handlers::domain_crud::update_many,
        crate::routes::crud_handlers::domain_crud::delete,
        crate::routes::crud_handlers::domain_crud::list,
        crate::routes::crud_handlers::domain_crud::page,
        // Application CRUD handlers
        crate::routes::crud_handlers::application_crud::create,
        crate::routes::crud_handlers::application_crud::create_many,
        crate::routes::crud_handlers::application_crud::get,
        crate::routes::crud_handlers::application_crud::update,
        crate::routes::crud_handlers::application_crud::update_many,
        crate::routes::crud_handlers::application_crud::delete,
        crate::routes::crud_handlers::application_crud::list,
        crate::routes::crud_handlers::application_crud::page,
        // Module CRUD handlers
        crate::routes::crud_handlers::module_crud::create,
        crate::routes::crud_handlers::module_crud::create_many,
        crate::routes::crud_handlers::module_crud::get,
        crate::routes::crud_handlers::module_crud::update,
        crate::routes::crud_handlers::module_crud::update_many,
        crate::routes::crud_handlers::module_crud::delete,
        crate::routes::crud_handlers::module_crud::list,
        crate::routes::crud_handlers::module_crud::page,
        // SysDatasource CRUD handlers
        // crate::routes::crud_handlers::sys_datasource_crud::create,
        // crate::routes::crud_handlers::sys_datasource_crud::create_many,
        crate::routes::crud_handlers::sys_datasource_crud::get,
        // crate::routes::crud_handlers::sys_datasource_crud::update,
        // crate::routes::crud_handlers::sys_datasource_crud::update_many,
        // crate::routes::crud_handlers::sys_datasource_crud::delete,
        crate::routes::crud_handlers::sys_datasource_crud::list,
        crate::routes::crud_handlers::sys_datasource_crud::page,

        // Domain handlers
        // crate::handlers::domain::handler::get_by_name,
        // crate::handlers::domain::handler::batch_create,
        // crate::handlers::domain::handler::search,
        // crate::handlers::domain::handler::count_by_status,

        // SysDatasource 自定义 handlers
        crate::handlers::sys_datasource::handler::get_by_db_id,
        crate::handlers::sys_datasource::handler::create_datasource,
        crate::handlers::sys_datasource::handler::update_datasource,
        crate::handlers::sys_datasource::handler::delete_datasource,
        crate::handlers::sys_datasource::handler::test_connection,
        crate::handlers::sys_datasource::handler::list_registered,
        // Plugin handlers
        crate::handlers::plugin::handler::plugin_install,
        crate::handlers::plugin::handler::plugin_uninstall,
        crate::handlers::plugin::handler::plugin_upgrade,
        crate::handlers::plugin::handler::plugin_downgrade,
        crate::handlers::plugin::handler::plugin_list,
        crate::handlers::plugin::handler::plugin_page,
        crate::handlers::plugin::handler::plugin_get,
    ),
    components(
        schemas(
            crate::handlers::domain::Domain,
            crate::handlers::domain::DomainForCreate,
            crate::handlers::domain::DomainForUpdate,
            crate::handlers::application::Application,
            crate::handlers::application::ApplicationForCreate,
            crate::handlers::application::ApplicationForUpdate,
            crate::handlers::module::Module,
            crate::handlers::module::ModuleForCreate,
            crate::handlers::module::ModuleForUpdate,
            crate::handlers::sys_datasource::SysDatasource,
            crate::handlers::sys_datasource::SysDatasourceForCreate,
            crate::handlers::sys_datasource::SysDatasourceForUpdate,
            crate::handlers::domain::handler::GetByNameParams,
            crate::handlers::domain::handler::BatchCreateParams,
            crate::handlers::domain::handler::SearchParams,
            crate::handlers::sys_datasource::handler::GetByDbIdParams,
            crate::handlers::sys_datasource::handler::DatasourceUpdatePayload,
            crate::handlers::sys_datasource::handler::DatasourceDeletePayload,
            crate::handlers::plugin::request::PluginInstallRequest,
            crate::handlers::plugin::request::PluginUninstallRequest,
            crate::handlers::plugin::request::PluginUpgradeRequest,
            crate::handlers::plugin::request::PluginDowngradeRequest,
            crate::handlers::plugin::request::PluginSourceRequest,
            crate::handlers::plugin::response::PluginInfoResponse,
            crate::handlers::plugin::response::PluginListResponse,
            crate::handlers::plugin::response::InstallResponse,
            crate::handlers::plugin::response::UninstallResponse,
            crate::handlers::plugin::response::UpgradeResponse,
            crate::handlers::plugin::response::DowngradeResponse,
            crate::api_response::Pagination,
        )
    )
)]
pub struct ApiDoc;
