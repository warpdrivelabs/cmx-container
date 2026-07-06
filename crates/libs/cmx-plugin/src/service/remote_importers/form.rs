//! `RemoteFormDefinitionImporter` — 表单定义的远程(gRPC)实现。

use async_trait::async_trait;
use cmx_core::model::module::FormDefinition;
use cmx_traits::error::TraitError;
use cmx_traits::resource::FormDefinitionImporter;
use cmx_traits::resource::{ResourceDataCategory, ResourceDataImportRequest};
use tracing::info;

use super::RemoteImporterContext;

/// 远程表单定义导入器(经 gRPC 或 HTTP 调用表单中心)。
pub struct RemoteFormDefinitionImporter {
    ctx: RemoteImporterContext,
}

impl RemoteFormDefinitionImporter {
    /// 创建远程表单定义导入器。
    pub fn new(ctx: RemoteImporterContext) -> Self {
        Self { ctx }
    }
}

#[async_trait]
impl FormDefinitionImporter for RemoteFormDefinitionImporter {
    async fn apply_form_definitions(
        &self,
        domain_code: &str,
        app_code: &str,
        module_code: &str,
        definitions: &[FormDefinition],
    ) -> Result<usize, TraitError> {
        if definitions.is_empty() {
            return Ok(0);
        }
        // 结构体 → ZIP(form_0.json, form_1.json ...)
        let zip_data = crate::center_client::packer::pack_definitions_to_zip(definitions, "form")
            .map_err(|e| TraitError::Business(format!("打包表单定义失败: {e}")))?;

        let req = ResourceDataImportRequest {
            category: ResourceDataCategory::Form,
            domain_code: domain_code.to_string(),
            application_code: app_code.to_string(),
            module_code: module_code.to_string(),
            plugin_id: String::new(),
            app_id: String::new(),
            version: String::new(),
            zip_data,
        };

        // 统一发送(按 ctx.config.mode 透明走 gRPC 或 HTTP)
        let result = self
            .ctx
            .send(crate::center_client::types::DataCategory::Form, req)
            .await
            .map_err(|e| TraitError::Business(e.to_string()))?;
        info!(
            created = result.created_count,
            updated = result.updated_count,
            "远程表单导入完成"
        );
        Ok((result.created_count + result.updated_count) as usize)
    }

    async fn list_form_definitions(
        &self,
        module_code: &str,
    ) -> Result<Vec<FormDefinition>, TraitError> {
        let req = ResourceDataImportRequest {
            category: ResourceDataCategory::Form,
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
            .send_list(crate::center_client::types::DataCategory::Form, req)
            .await
            .map_err(|e| TraitError::Business(e.to_string()))?;
        serde_json::from_slice(&result.json_data)
            .map_err(|e| TraitError::Business(format!("反序列化远程表单定义失败: {e}")))
    }
}
