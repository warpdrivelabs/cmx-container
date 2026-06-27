//! 预签名 URL 生成
//!
//! 实现 [`StorageService::presign_download`] 与 [`StorageService::presign_upload`]，
//! 用于生成短期有效的上传/下载 URL，客户端可直接通过该 URL 与存储后端交互。

use std::time::Duration;

use cmx_database::crud::GenericCrudService;
use tracing::info;

use crate::bmc::*;
use crate::error::{Error, Result};
use crate::path_gen::{extract_extension, generate_storage_path};
use crate::service::DefaultStorageService;
use crate::types::*;

impl DefaultStorageService {
    /// 生成下载预签名 URL（[`crate::service::StorageService::presign_download`] 的实现）。
    pub(super) async fn presign_download(&self, file_id: &str, expires: Duration) -> Result<String> {
        let detail = self.find_file_detail(file_id).await?;

        let path = detail.path.as_deref()
            .ok_or_else(|| Error::PresignError(format!("文件 {} 无存储路径", file_id)))?;

        let backend = self.manager.get_backend(detail.platform.as_deref())?;
        let url = backend.presign_read(path, expires).await?;

        info!(file_id = file_id, expires_secs = expires.as_secs(), "生成下载预签名 URL");
        Ok(url)
    }

    /// 生成上传预签名 URL（[`crate::service::StorageService::presign_upload`] 的实现）。
    pub(super) async fn presign_upload(&self, request: PresignUploadRequest, expires: Duration) -> Result<PresignUploadResult> {
        let platform = request.platform.clone();
        let backend = self.manager.get_backend(platform.as_deref())?;
        let config = self.manager.get_config(backend.platform())
            .ok_or_else(|| Error::ConfigError(format!("找不到平台配置: {}", backend.platform())))?;

        let ext = extract_extension(&request.filename);
        let (storage_path, _filename) = generate_storage_path(&config.base_path, None, &ext, &config.storage_type);

        let url = backend.presign_write(&storage_path, expires).await?;

        let file_id = uuid::Uuid::new_v4().to_string();
        let access_url = config.get_access_url(&storage_path);

        let (mm, db_id) = self.get_db().await?;
        let data = FileDetailForCreate {
            id: Some(file_id.clone()),
            url: Some(access_url),
            path: Some(storage_path),
            platform: Some(backend.platform().to_string()),
            original_filename: Some(request.filename),
            content_type: request.content_type,
            ext: Some(if ext.starts_with('.') { ext.clone() } else { format!(".{}", ext) }),
            upload_status: Some(0),
            ..Default::default()
        };

        GenericCrudService::<FileDetailBmc>::create(mm, &db_id, None, data)
            .await
            .map_err(Error::from)?;

        info!(file_id = %file_id, expires_secs = expires.as_secs(), "生成上传预签名 URL");
        Ok(PresignUploadResult { url, file_id })
    }
}
