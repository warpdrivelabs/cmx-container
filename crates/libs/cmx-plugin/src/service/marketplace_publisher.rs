//! 市场发布器模块
//!
//! 封装从已部署的 ZIP 包发布到插件市场的完整逻辑。

use std::path::PathBuf;

use cmx_core::model::meta::plugin::PluginDefinition;
use serde::{Deserialize, Serialize};

use crate::error::{PluginError, PluginResult};
use crate::marketplace::model::{MarketplacePluginForCreate, MarketplacePluginVersionForCreate};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishFromDeployRequest {
    pub plugin_id: String,
    pub version: String,
    pub plugin_def: PluginDefinition,
    pub zip_file_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishFromDeployResult {
    pub marketplace_plugin_id: String,
    pub marketplace_version_id: String,
    pub storage_file_id: String,
    pub is_new_plugin: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplacePublishInfo {
    pub marketplace_plugin_id: String,
    pub marketplace_version_id: String,
    pub storage_file_id: String,
    pub is_new_plugin: bool,
}

impl From<PublishFromDeployResult> for MarketplacePublishInfo {
    fn from(r: PublishFromDeployResult) -> Self {
        Self {
            marketplace_plugin_id: r.marketplace_plugin_id,
            marketplace_version_id: r.marketplace_version_id,
            storage_file_id: r.storage_file_id,
            is_new_plugin: r.is_new_plugin,
        }
    }
}

pub struct MarketplacePublisher;

impl MarketplacePublisher {
    pub async fn publish_from_deploy(req: &PublishFromDeployRequest) -> PluginResult<PublishFromDeployResult> {
        let existing = {
            let service = crate::marketplace::service::get_marketplace_service().await;
            service.repo().get_plugin_by_plugin_id(&req.plugin_id).await?
        };
        let is_new_plugin = existing.is_none();

        let zip_bytes = tokio::fs::read(&req.zip_file_path)
            .await
            .map_err(|e| PluginError::Plugin(format!("读取 ZIP 文件失败: {}", e)))?;

        let file_name = req
            .zip_file_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| format!("plugin-{}-{}.zip", req.plugin_id, req.version));

        let storage_service = cmx_storage::global::GlobalStorageService::get().service();
        let upload_request = cmx_storage::types::UploadRequest {
            data: zip_bytes.into(),
            original_filename: Some(file_name),
            content_type: Some("application/zip".to_string()),
            object_type: Some("marketplace_plugin".to_string()),
            object_id: Some(req.plugin_id.clone()),
            platform: None,
            user_metadata: None,
            acl: None,
        };
        let file_info = storage_service
            .upload(upload_request)
            .await
            .map_err(|e| PluginError::Plugin(format!("上传插件包到存储失败: {}", e)))?;

        tracing::info!(
            "插件包已上传到 cmx-storage: file_id={}, size={}",
            file_info.id,
            file_info.size
        );

        let plugin_req = MarketplacePluginForCreate {
            plugin_id: req.plugin_id.clone(),
            name: Some(req.plugin_def.name.clone()),
            description: req.plugin_def.description.clone(),
            short_description: None,
            icon_url: None,
            category: None,
            tags: None,
            vendor_name: req.plugin_def.vendor_name.clone(),
            vendor_url: req.plugin_def.vendor_url.clone(),
            vendor_contact: req.plugin_def.vendor_contact.clone(),
            license_type: None,
            homepage_url: None,
            documentation_url: None,
            repository_url: None,
            status: Some("published".to_string()),
            is_featured: None,
            is_official: None,
            domain_code: req.plugin_def.domain_code.clone(),
            application_code: req.plugin_def.application_code.clone(),
            module_code: req.plugin_def.module_code.clone(),
            plugin_type: Some(req.plugin_def.r#type.clone()),
        };

        let version_req = MarketplacePluginVersionForCreate {
            plugin_id: req.plugin_id.clone(),
            version: req.version.clone(),
            version_rank: Some(0),
            changelog: None,
            release_notes: None,
            download_url: Some(file_info.url),
            storage_file_id: Some(file_info.id.clone()),
            package_size: Some(file_info.size),
            checksum: file_info.hash_info,
            min_platform_version: None,
            max_platform_version: None,
            dependencies: None,
            compatibility: None,
            status: Some("published".to_string()),
            is_latest: Some(1),
            is_stable: Some(1),
            allow_version_overwrite: true,
        };

        let service = crate::marketplace::service::get_marketplace_service().await;
        let plugin = service.publish_plugin(plugin_req, version_req).await?;

        let version_info = service
            .repo()
            .get_version(&req.plugin_id, &req.version)
            .await?
            .ok_or_else(|| PluginError::Plugin("发布后未找到版本记录".to_string()))?;

        Ok(PublishFromDeployResult {
            marketplace_plugin_id: plugin.id,
            marketplace_version_id: version_info.id,
            storage_file_id: file_info.id,
            is_new_plugin,
        })
    }
}
