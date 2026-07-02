//! 文件上传
//!
//! 实现 [`StorageService::upload`]，包含秒传检测、MIME 检测、存储路径生成、
//! 缩略图生成与上传、文件记录入库等完整流程。

use tracing::info;

use crate::error::{Error, Result};
use crate::mime_detect::detect_mime;
use crate::path_gen::{extract_extension, generate_storage_path};
use crate::service::DefaultStorageService;
use crate::types::*;

impl DefaultStorageService {
    /// 文件上传（秒传检测、MIME 检测、存储路径生成、缩略图、文件记录入库）。
    ///
    /// 为 [`crate::service::StorageService::upload`] 的实现，逻辑集中在本固有方法中。
    pub(super) async fn upload(&self, request: UploadRequest) -> Result<FileInfo> {
        // ── 第一步：计算文件哈希，用于秒传检测 ──
        let md5_hash = Self::compute_md5(&request.data);
        let hash_info = serde_json::json!({"md5": md5_hash}).to_string();

        // 确定目标存储平台：请求指定 > 默认平台
        let platform = request
            .platform
            .clone()
            .or_else(|| self.manager.get_default_platform().map(|s| s.to_string()))
            .unwrap_or_default();

        // ── 第二步：秒传检测，相同 MD5 + 相同平台则复用已有记录 ──
        if let Some(existing) = self.try_instant_upload(&hash_info, &platform).await? {
            info!(hash = %md5_hash, existing_id = %existing.id, "秒传命中");
            let mut file_info = existing.clone();
            file_info.id = uuid::Uuid::new_v4().to_string();
            file_info.original_filename = request.original_filename.clone();
            file_info.object_id = request.object_id.clone();
            file_info.object_type = request.object_type.clone();
            return self
                .create_file_record(&file_info, &request, &md5_hash)
                .await
                .map(|d| d.to_file_info());
        }

        // ── 第三步：获取存储后端和配置 ──
        let backend = self.manager.get_backend(Some(&platform))?;
        let config = self
            .manager
            .get_config(backend.platform())
            .ok_or_else(|| Error::ConfigError(format!("找不到平台配置: {}", backend.platform())))?;

        // ── 第四步：检测 MIME 类型和扩展名 ──
        let content_type = detect_mime(
            &request.data,
            request.original_filename.as_deref(),
            request.content_type.as_deref(),
        );

        let ext = request
            .original_filename
            .as_deref()
            .map(extract_extension)
            .unwrap_or_default();

        // 根据存储类型生成路径：Local 用 yyyyMM，S3 用 yyyy/MM/dd
        let (storage_path, filename) = generate_storage_path(
            &config.base_path,
            request.object_type.as_deref(),
            &ext,
            &config.storage_type,
        );

        // ── 第五步：上传原始文件到存储后端 ──
        let write_opts = WriteOptions {
            content_type: Some(content_type.clone()),
            content_disposition: None,
            cache_control: None,
            user_metadata: request.user_metadata.clone(),
            acl: request.acl.clone(),
        };

        backend
            .write(&storage_path, request.data.clone(), write_opts)
            .await?;

        let access_url = config.get_access_url(&storage_path);
        let file_id = uuid::Uuid::new_v4().to_string();

        // ── 第六步：如果是图片，生成缩略图并上传到同目录 ──
        // 缩略图命名规则：{原图文件名}.min.jpg，与原图存放在同一目录
        let mut th_url = None;
        let mut th_filename = None;
        let mut th_size = None;
        let mut th_content_type = None;

        if let Some(thumbnail) = Self::generate_thumbnail(&request.data, &content_type) {
            let th_name = format!("{}.min.jpg", filename);
            let th_storage_path = format!(
                "{}/{}",
                storage_path
                    .rfind('/')
                    .map(|i| &storage_path[..i])
                    .unwrap_or(""),
                th_name
            );
            let th_storage_path = th_storage_path.trim_start_matches('/');

            let th_write_opts = WriteOptions {
                content_type: Some("image/jpeg".to_string()),
                content_disposition: None,
                cache_control: None,
                user_metadata: None,
                acl: None,
            };

            match backend
                .write(th_storage_path, thumbnail.data.clone(), th_write_opts)
                .await
            {
                Ok(_) => {
                    info!("缩略图上传成功，大小: {} bytes", thumbnail.data.len());
                    th_url = Some(config.get_access_url(th_storage_path));
                    th_filename = Some(th_name);
                    th_size = Some(thumbnail.data.len() as i64);
                    th_content_type = Some("image/jpeg".to_string());
                }
                Err(e) => {
                    // 缩略图上传失败不影响主文件，仅记录 warn 日志
                    tracing::warn!("缩略图上传失败（不影响主文件）: {}", e);
                }
            }
        }

        // ── 第七步：组装 FileInfo 并一次性写入数据库（包含缩略图信息） ──
        let file_info = FileInfo {
            id: file_id.clone(),
            url: access_url,
            size: request.data.len() as i64,
            filename,
            original_filename: request.original_filename.clone(),
            base_path: Some(config.base_path.clone()),
            path: Some(storage_path.clone()),
            ext: Some(if ext.starts_with('.') {
                ext.clone()
            } else {
                format!(".{}", ext)
            }),
            content_type: Some(content_type),
            platform: backend.platform().to_string(),
            th_url,
            th_filename,
            th_size,
            th_content_type,
            object_id: request.object_id.clone(),
            object_type: request.object_type.clone(),
            user_metadata: request
                .user_metadata
                .as_ref()
                .map(|m| serde_json::to_string(m).unwrap_or_default()),
            hash_info: Some(hash_info),
            upload_id: None,
            upload_status: Some(0),
            create_time: Some(chrono::Local::now().naive_utc()),
        };

        self.create_file_record(&file_info, &request, &md5_hash)
            .await?;

        info!(file_id = %file_info.id, platform = %file_info.platform, size = file_info.size, "文件上传成功");

        Ok(file_info)
    }
}
