//! cmx-plugin-api 的 OpenApi 切片。
//!
//! 从 cmx-api/openapi.rs 迁入的 plugin/table_metadata/marketplace 相关 paths + schemas，
//! 由 platform-app 用 `OpenApi::merge()` 聚合。

use utoipa::OpenApi;

/// 插件 / 市场 / 表元数据 OpenApi 切片。
#[derive(OpenApi)]
#[openapi(
    paths(
        // Plugin handlers
        crate::handlers::plugin::handler::plugin_install,
        crate::handlers::plugin::handler::plugin_uninstall,
        crate::handlers::plugin::handler::plugin_upgrade,
        crate::handlers::plugin::handler::plugin_downgrade,
        crate::handlers::plugin::handler::plugin_list,
        crate::handlers::plugin::handler::plugin_page,
        crate::handlers::plugin::handler::plugin_get,
        crate::handlers::plugin::handler::plugin_deploy,
        crate::handlers::plugin::handler::plugin_exists,
        crate::handlers::plugin::handler::plugin_functions,
        // TableMetadata handlers
        crate::handlers::table_metadata::handler::table_metadata_list,
        crate::handlers::table_metadata::handler::table_metadata_page,
        crate::handlers::table_metadata::handler::table_metadata_get_by_id,
        crate::handlers::table_metadata::handler::table_metadata_get_by_name,
        crate::handlers::table_metadata::handler::table_metadata_exists,
        // Module 迁移包导入/导出
        crate::handlers::module::package_handler::module_package_import,
        crate::handlers::module::package_handler::module_package_export,
        // Marketplace handlers
        crate::handlers::marketplace::handler::marketplace_plugin_page,
        crate::handlers::marketplace::handler::marketplace_plugin_get_by_id,
        crate::handlers::marketplace::handler::marketplace_plugin_publish,
        crate::handlers::marketplace::handler::marketplace_plugin_update,
        crate::handlers::marketplace::handler::marketplace_plugin_delete,
        crate::handlers::marketplace::handler::marketplace_plugin_version_list,
        crate::handlers::marketplace::handler::marketplace_plugin_version_get_by_id,
        crate::handlers::marketplace::handler::marketplace_plugin_install,
        crate::handlers::marketplace::handler::marketplace_plugin_rate,
        crate::handlers::marketplace::handler::marketplace_plugin_rating_list,
        crate::handlers::marketplace::handler::marketplace_category_list,
        crate::handlers::marketplace::handler::marketplace_trending_list,
        crate::handlers::marketplace::handler::marketplace_plugin_upgrade,
        crate::handlers::marketplace::handler::marketplace_plugin_check_updates,
        crate::handlers::marketplace::handler::marketplace_plugin_download,
    ),
    components(
        schemas(
            // Plugin request/response
            crate::handlers::plugin::request::PluginInstallRequest,
            crate::handlers::plugin::request::PluginUninstallRequest,
            crate::handlers::plugin::request::PluginUpgradeRequest,
            crate::handlers::plugin::request::PluginDowngradeRequest,
            crate::handlers::plugin::request::PluginSourceRequest,
            crate::handlers::plugin::request::PluginDeployRequest,
            crate::handlers::plugin::request::PluginFunctionsRequest,
            crate::handlers::plugin::response::PluginInfoResponse,
            crate::handlers::plugin::response::PluginListResponse,
            crate::handlers::plugin::response::InstallResponse,
            crate::handlers::plugin::response::UninstallResponse,
            crate::handlers::plugin::response::UpgradeResponse,
            crate::handlers::plugin::response::DowngradeResponse,
            crate::handlers::plugin::response::PluginDeployResponse,
            crate::handlers::plugin::response::PluginFunctionsResponse,
            // Marketplace request/response
            crate::handlers::marketplace::request::PublishPluginRequest,
            crate::handlers::marketplace::request::UpdateMarketplacePluginRequest,
            crate::handlers::marketplace::request::DeleteMarketplacePluginRequest,
            crate::handlers::marketplace::request::MarketInstallRequest,
            crate::handlers::marketplace::request::RatePluginRequest,
            crate::handlers::marketplace::request::TrendingFilter,
            crate::handlers::marketplace::request::MarketplacePluginFilterDoc,
            crate::handlers::marketplace::request::MarketplacePluginVersionFilterDoc,
            crate::handlers::marketplace::request::MarketplaceRatingFilterDoc,
            crate::handlers::marketplace::response::MarketplacePluginResponse,
            crate::handlers::marketplace::response::MarketplaceVersionResponse,
            crate::handlers::marketplace::response::MarketplacePluginDetailResponse,
            crate::handlers::marketplace::response::MarketInstallResponse,
            crate::handlers::marketplace::response::MarketplaceRatingResponse,
            crate::handlers::marketplace::response::CategoryResponse,
            // Module 迁移包 schema
            crate::handlers::module::package_handler::ModuleImportResponse,
        )
    )
)]
pub struct PluginApiDoc;
