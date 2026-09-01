//! `RemotePermissionDefinitionImporter` — 权限定义的远程(gRPC)实现。
//!
//! 权限的远程导入走 cmx-iam 既有的 `PermissionFile` ZIP 格式(permdata/*.json),
//! 与 ResourceDataImporterImpl 的 Perm 类别处理对齐。

use async_trait::async_trait;
use cmx_core::model::iam::{PermissionDefinition, PermissionFile};
use cmx_traits::error::TraitError;
use cmx_traits::resource::PermissionDefinitionImporter;
use cmx_traits::resource::{ResourceDataCategory, ResourceDataImportRequest};
use tracing::info;

use super::{RemoteImporterContext, plugin_err_to_trait};

/// 远程权限定义导入器(经 gRPC 或 HTTP 调用权限中心)。
pub struct RemotePermissionDefinitionImporter {
    ctx: RemoteImporterContext,
}

impl RemotePermissionDefinitionImporter {
    /// 创建远程权限定义导入器。
    pub fn new(ctx: RemoteImporterContext) -> Self {
        Self { ctx }
    }
}

#[async_trait]
impl PermissionDefinitionImporter for RemotePermissionDefinitionImporter {
    async fn apply_permission_definitions(
        &self,
        domain_code: &str,
        app_code: &str,
        module_code: &str,
        definitions: &[PermissionDefinition],
    ) -> Result<usize, TraitError> {
        if definitions.is_empty() {
            return Ok(0);
        }
        // 权限打包为 PermissionFile JSON 单文件 ZIP(对齐 cmx-iam permdata 格式)
        let perm_file = PermissionFile {
            name: format!("{}_permissions", module_code),
            version: "1.0.0".to_string(),
            description: format!("模块 {} 权限定义", module_code),
            permissions: definitions.to_vec(),
        };
        let zip_data = crate::service::remote_importers::packer::pack_payload_to_zip(
            &perm_file,
            &format!("{module_code}_permissions.json"),
        )
        .map_err(|e| TraitError::Business(format!("打包权限定义失败: {e}")))?;

        let req = ResourceDataImportRequest {
            category: ResourceDataCategory::Perm,
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
            .send(crate::service::remote_importers::types::DataCategory::Perm, req)
            .await
            .map_err(plugin_err_to_trait)?;
        info!(
            created = result.created_count,
            updated = result.updated_count,
            "远程权限导入完成"
        );
        Ok((result.created_count + result.updated_count) as usize)
    }

    async fn list_permission_definitions(
        &self,
        domain_code: &str,
        app_code: &str,
        module_code: &str,
    ) -> Result<Vec<PermissionDefinition>, TraitError> {
        let req = ResourceDataImportRequest {
            category: ResourceDataCategory::Perm,
            domain_code: domain_code.to_string(),
            application_code: app_code.to_string(),
            module_code: module_code.to_string(),
            plugin_id: String::new(),
            app_id: String::new(),
            version: String::new(),
            zip_data: Vec::new(),
        };
        let result = self
            .ctx
            .send_list(crate::service::remote_importers::types::DataCategory::Perm, req)
            .await
            .map_err(plugin_err_to_trait)?;
        serde_json::from_slice(&result.json_data)
            .map_err(|e| TraitError::Business(format!("反序列化远程权限定义失败: {e}")))
    }
}
