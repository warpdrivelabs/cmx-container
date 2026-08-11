//! `CmxAppState` 组装
//!
//! 把各子系统的 trait 实例注入 `CmxAppState`，产出注入完成的应用状态供路由使用。

use std::sync::Arc;

use cmx_api::CmxAppState;
use cmx_api::app_state::IamState;
use cmx_service::{GlobalServiceQuery, GlobalServiceStorage};
use cmx_traits::auth::AuthService;
use cmx_traits::resource::{DefinitionImporterBundle, ResourceDataImporter};

/// 组装完整的 `CmxAppState`。
///
/// 注入插件查询、运行时 invoker、服务查询/存储、存储服务、认证服务与 IAM 状态；
/// `resource_data_importer`（HTTP `/iam/permissions/import` 和 `/cleanup` 用）与
/// `definition_importers`（模块导入/导出复用统一导入器集合）在 `Some` 时条件注入——
/// 与抽取前 `main` 内联逻辑逐行一致。
pub fn build_app_state(
    auth_service: Arc<dyn AuthService>,
    iam_state: Arc<IamState>,
    resource_data_importer: Option<Arc<dyn ResourceDataImporter>>,
    definition_importers: Option<Arc<DefinitionImporterBundle>>,
) -> CmxAppState {
    let app_state = CmxAppState::new()
        .with_plugin_query(cmx_plugin::GlobalPluginManager::get_as_plugin_query())
        .with_runtime_invoker(cmx_runtime::GlobalExtismEngine::get_as_invoker())
        .with_service_query(GlobalServiceQuery::get().clone())
        .with_service_storage(GlobalServiceStorage::get().clone())
        .with_storage_service(
            cmx_storage::global::GlobalStorageService::get()
                .service()
                .clone(),
        )
        .with_auth_service(auth_service)
        .with_iam(iam_state);

    // 注入 ResourceDataImporter（HTTP 端点 /iam/permissions/import 和 /cleanup 使用）
    let app_state = if let Some(importer) = resource_data_importer {
        app_state.with_resource_data_importer(importer)
    } else {
        app_state
    };

    // 注入 DefinitionImporterBundle（模块导入/导出复用统一导入器集合）
    if let Some(importers) = definition_importers {
        app_state.with_definition_importers(importers)
    } else {
        app_state
    }
}
