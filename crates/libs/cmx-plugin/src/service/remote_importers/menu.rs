//! `RemoteMenuDefinitionImporter` — 菜单定义的远程(gRPC)实现。

use async_trait::async_trait;
use cmx_core::model::module::MenuDefinition;
use cmx_traits::error::TraitError;
use cmx_traits::resource::MenuDefinitionImporter;
use cmx_traits::resource::{ResourceDataCategory, ResourceDataImportRequest};
use tracing::info;

use super::{RemoteImporterContext, plugin_err_to_trait};

/// 远程菜单定义导入器(经 gRPC 或 HTTP 调用门户中心)。
pub struct RemoteMenuDefinitionImporter {
    ctx: RemoteImporterContext,
}

impl RemoteMenuDefinitionImporter {
    /// 创建远程菜单定义导入器。
    pub fn new(ctx: RemoteImporterContext) -> Self {
        Self { ctx }
    }
}

#[async_trait]
impl MenuDefinitionImporter for RemoteMenuDefinitionImporter {
    async fn apply_menu_definitions(
        &self,
        domain_code: &str,
        app_code: &str,
        module_code: &str,
        definitions: &[MenuDefinition],
    ) -> Result<usize, TraitError> {
        if definitions.is_empty() {
            return Ok(0);
        }
        // 结构体 → ZIP(menu_0.json, menu_1.json ...)
        let zip_data = crate::service::remote_importers::packer::pack_definitions_to_zip(definitions, "menu")
            .map_err(|e| TraitError::Business(format!("打包菜单定义失败: {e}")))?;

        let req = ResourceDataImportRequest {
            category: ResourceDataCategory::Menu,
            domain_code: domain_code.to_string(),
            application_code: app_code.to_string(),
            module_code: module_code.to_string(),
            plugin_id: String::new(),
            app_id: String::new(),
            version: String::new(),
            zip_data,
        };

        let result = self
            .ctx
            .send(crate::service::remote_importers::types::DataCategory::Menu, req)
            .await
            .map_err(plugin_err_to_trait)?;
        info!(
            created = result.created_count,
            updated = result.updated_count,
            "远程菜单导入完成"
        );
        Ok((result.created_count + result.updated_count) as usize)
    }

    async fn list_menu_definitions(
        &self,
        module_code: &str,
    ) -> Result<Vec<MenuDefinition>, TraitError> {
        let req = ResourceDataImportRequest {
            category: ResourceDataCategory::Menu,
            domain_code: String::new(),
            application_code: String::new(),
            module_code: module_code.to_string(),
            plugin_id: String::new(),
            app_id: String::new(),
            version: String::new(),
            zip_data: Vec::new(),
        };
        let result = self
            .ctx
            .send_list(crate::service::remote_importers::types::DataCategory::Menu, req)
            .await
            .map_err(plugin_err_to_trait)?;
        serde_json::from_slice(&result.json_data)
            .map_err(|e| TraitError::Business(format!("反序列化远程菜单定义失败: {e}")))
    }
}
