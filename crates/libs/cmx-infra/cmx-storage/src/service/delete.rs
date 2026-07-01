//! 文件删除
//!
//! 实现 [`StorageService::delete`] 与 [`StorageService::batch_delete`]。
//!
//! `delete` 采用归档策略：物理文件保留，仅将数据库记录的 `archived` 标记为 1。

use cmx_database::crud::GenericCrudService;
use serde_json::Value;
use tracing::info;

use crate::bmc::*;
use crate::error::{Error, Result};
use crate::service::DefaultStorageService;

impl DefaultStorageService {
    /// 删除文件（归档策略，[`crate::service::StorageService::delete`] 的实现）。
    pub(super) async fn delete(&self, file_id: &str) -> Result<()> {
        let detail = self.find_file_detail(file_id).await?;

        if let Some(path) = detail.path.as_deref() {
            let platform = detail.platform.as_deref().unwrap_or("");
            if let Ok(backend) = self.manager.get_backend(Some(platform)) {
                let _ = backend.delete(path).await;
            }
        }

        let (mm, db_id) = self.get_db().await?;
        let update_data = FileDetailForUpdate {
            archived: Some(1),
            url: None,
            size: None,
            filename: None,
            upload_status: None,
            th_url: None,
            th_filename: None,
            th_size: None,
            th_content_type: None,
        };
        GenericCrudService::<FileDetailBmc>::update(
            mm, &db_id, None, Value::String(file_id.to_string()), update_data,
        )
            .await
            .map_err(Error::from)?;

        info!(file_id = file_id, "文件已删除（归档）");
        Ok(())
    }

    /// 批量删除（[`crate::service::StorageService::batch_delete`] 的实现）。
    pub(super) async fn batch_delete(&self, file_ids: &[String]) -> Result<Vec<Result<()>>> {
        let mut results = Vec::new();
        for file_id in file_ids {
            results.push(self.delete(file_id).await);
        }
        Ok(results)
    }
}
