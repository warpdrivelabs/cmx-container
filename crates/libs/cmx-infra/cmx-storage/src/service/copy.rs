//! 文件复制
//!
//! 实现 [`StorageService::copy_file`]。
//!
//! 支持跨平台复制：相同平台使用原生 `copy`，不同平台使用读取后写入。

use cmx_database::crud::GenericCrudService;
use tracing::info;

use crate::bmc::*;
use crate::error::{Error, Result};
use crate::path_gen::generate_storage_path;
use crate::service::DefaultStorageService;
use crate::types::*;

impl DefaultStorageService {
    /// 复制文件（[`crate::service::StorageService::copy_file`] 的实现，支持跨平台）。
    pub(super) async fn copy_file(
        &self,
        file_id: &str,
        target_platform: Option<&str>,
    ) -> Result<FileInfo> {
        let detail = self.find_file_detail(file_id).await?;

        let src_path = detail
            .path
            .as_deref()
            .ok_or_else(|| Error::CopyError(format!("源文件 {} 无存储路径", file_id)))?;

        let src_platform = detail.platform.as_deref().unwrap_or("");
        let src_backend = self.manager.get_backend(Some(src_platform))?;

        let target_backend = self.manager.get_backend(target_platform)?;
        let target_config = self
            .manager
            .get_config(target_backend.platform())
            .ok_or_else(|| {
                Error::ConfigError(format!("找不到目标平台配置: {}", target_backend.platform()))
            })?;

        let ext = detail.ext.as_deref().unwrap_or("");
        let (target_path, filename) = generate_storage_path(
            &target_config.base_path,
            detail.object_type.as_deref(),
            ext,
            &target_config.storage_type,
        );

        if src_backend.platform() == target_backend.platform() {
            src_backend.copy(src_path, &target_path).await?;
        } else {
            let data = src_backend.read(src_path).await?;
            let write_opts = WriteOptions {
                content_type: detail.content_type.clone(),
                content_disposition: None,
                cache_control: None,
                user_metadata: None,
                acl: None,
            };
            target_backend.write(&target_path, data, write_opts).await?;
        }

        let access_url = target_config.get_access_url(&target_path);
        let new_id = uuid::Uuid::new_v4().to_string();

        let (mm, db_id) = self.get_db().await?;
        let data = FileDetailForCreate {
            id: Some(new_id.clone()),
            url: Some(access_url),
            size: detail.size,
            filename: Some(filename),
            original_filename: detail.original_filename.clone(),
            base_path: Some(target_config.base_path.clone()),
            path: Some(target_path.clone()),
            ext: detail.ext.clone(),
            content_type: detail.content_type.clone(),
            platform: Some(target_backend.platform().to_string()),
            object_id: detail.object_id.clone(),
            object_type: detail.object_type.clone(),
            hash_info: detail.hash_info.clone(),
            upload_status: Some(0),
            ..Default::default()
        };

        let dataset = GenericCrudService::<FileDetailBmc>::create(mm, &db_id, None, data)
            .await
            .map_err(Error::from)?;

        let new_detail = Self::dataset_to_file_detail(&dataset)
            .ok_or_else(|| Error::CopyError("创建复制文件数据库记录失败".to_string()))?;

        info!(src_id = file_id, new_id = %new_detail.id, "文件复制成功");
        Ok(new_detail.to_file_info())
    }
}
