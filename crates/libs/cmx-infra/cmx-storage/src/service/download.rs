//! 文件下载
//!
//! 实现 [`StorageService::download`] 与 [`StorageService::download_thumbnail`]。

use crate::error::{Error, Result};
use crate::service::DefaultStorageService;
use crate::types::*;

impl DefaultStorageService {
    /// 文件下载（[`crate::service::StorageService::download`] 的实现）。
    pub(super) async fn download(&self, file_id: &str) -> Result<FileDownload> {
        let detail = self.find_file_detail(file_id).await?;
        let file_info = detail.to_file_info();

        let path = detail.path.as_deref()
            .ok_or_else(|| Error::DownloadError(format!("文件 {} 无存储路径", file_id)))?;

        let backend = self.manager.get_backend(detail.platform.as_deref())?;
        let data = backend.read(path).await?;

        let content_type = file_info.content_type.clone().unwrap_or_else(|| "application/octet-stream".to_string());
        let content_disposition = Self::build_content_disposition(
            file_info.original_filename.as_deref().unwrap_or(&file_info.filename)
        );

        Ok(FileDownload {
            data,
            file_info,
            content_type,
            content_disposition,
            content_length: detail.size.unwrap_or(0) as u64,
        })
    }

    /// 缩略图下载（[`crate::service::StorageService::download_thumbnail`] 的实现）。
    pub(super) async fn download_thumbnail(&self, file_id: &str) -> Result<FileDownload> {
        let detail = self.find_file_detail(file_id).await?;

        let src_path = detail.path.as_deref()
            .ok_or_else(|| Error::NotFoundError(format!("文件存储路径不存在: {}", file_id)))?;

        // 缩略图路径 = 原图路径 + ".min.jpg"
        let th_path = format!("{}.min.jpg", src_path);

        let backend = self.manager.get_backend(detail.platform.as_deref())?;
        let data = backend.read(&th_path).await?;

        let content_type = detail.th_content_type.clone().unwrap_or_else(|| "image/jpeg".to_string());
        let file_info = detail.to_file_info();

        Ok(FileDownload {
            data,
            file_info,
            content_type,
            content_disposition: format!("inline; filename=\"thumbnail_{}\"", file_id),
            content_length: detail.th_size.unwrap_or(0) as u64,
        })
    }
}
