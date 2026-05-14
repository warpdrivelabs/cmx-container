//! 存储服务层模块
//!
//! 提供组合存储后端和数据库操作的高级文件服务。
//! 参考 Java FileStorageService 的设计模式。

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use cmx_core::model::data::dataset::DataSet;
use cmx_database::crud::GenericCrudService;
use cmx_database::get_default_db_manager;
use modql::filter::{ListOptions, OpValString, OpValsString};
use serde_json::Value;
use tracing::info;
use urlencoding::encode;

use crate::bmc::*;
use crate::error::{Error, Result};
use crate::manager::StorageManager;
use crate::mime_detect::detect_mime;
use crate::path_gen::{extract_extension, generate_storage_path};
use crate::types::*;

/// 存储服务 trait
///
/// 面向业务的高级文件服务接口，组合后端 I/O 与数据库操作。
/// 提供完整的文件管理功能，包括上传、下载、删除、预签名和分片上传等。
#[async_trait]
pub trait StorageService: Send + Sync + 'static {
    /// 上传文件
    ///
    /// # Arguments
    ///
    /// * `request` - 上传请求
    ///
    /// # Returns
    ///
    /// 成功时返回文件信息。
    ///
    /// # Errors
    ///
    /// * 当存储后端操作失败时返回错误
    /// * 当数据库操作失败时返回错误
    async fn upload(&self, request: UploadRequest) -> Result<FileInfo>;

    /// 上传文件（含缩略图）
    ///
    /// # Arguments
    ///
    /// * `request` - 上传请求
    /// * `thumbnail` - 缩略图数据
    ///
    /// # Returns
    ///
    /// 成功时返回包含缩略图信息的文件信息。
    ///
    /// # Errors
    ///
    /// * 当存储后端操作失败时返回错误
    /// * 当数据库操作失败时返回错误
    async fn upload_with_thumbnail(&self, request: UploadRequest, thumbnail: ThumbnailData) -> Result<FileInfo>;

    /// 下载文件
    ///
    /// # Arguments
    ///
    /// * `file_id` - 文件唯一标识
    ///
    /// # Returns
    ///
    /// 成功时返回文件下载结果。
    ///
    /// # Errors
    ///
    /// * 当文件不存在时返回 `NotFoundError`
    /// * 当存储后端读取失败时返回 `DownloadError`
    async fn download(&self, file_id: &str) -> Result<FileDownload>;

    /// 下载缩略图
    ///
    /// # Arguments
    ///
    /// * `file_id` - 文件唯一标识
    ///
    /// # Returns
    ///
    /// 成功时返回缩略图下载结果。
    ///
    /// # Errors
    ///
    /// * 当文件或缩略图不存在时返回 `NotFoundError`
    /// * 当存储后端读取失败时返回 `DownloadError`
    async fn download_thumbnail(&self, file_id: &str) -> Result<FileDownload>;

    /// 删除文件
    ///
    /// 将文件标记为已删除（归档），物理文件保留。
    ///
    /// # Arguments
    ///
    /// * `file_id` - 文件唯一标识
    ///
    /// # Returns
    ///
    /// 成功时返回 `Ok(())`。
    ///
    /// # Errors
    ///
    /// * 当文件不存在时返回 `NotFoundError`
    /// * 当数据库操作失败时返回错误
    async fn delete(&self, file_id: &str) -> Result<()>;

    /// 批量删除
    ///
    /// # Arguments
    ///
    /// * `file_ids` - 文件唯一标识列表
    ///
    /// # Returns
    ///
    /// 返回每个文件删除操作的结果列表。
    async fn batch_delete(&self, file_ids: &[String]) -> Result<Vec<Result<()>>>;

    /// 获取文件信息
    ///
    /// # Arguments
    ///
    /// * `file_id` - 文件唯一标识
    ///
    /// # Returns
    ///
    /// 成功时返回文件信息。
    ///
    /// # Errors
    ///
    /// * 当文件不存在时返回 `NotFoundError`
    async fn get_file_info(&self, file_id: &str) -> Result<FileInfo>;

    /// 分页查询文件列表
    ///
    /// # Arguments
    ///
    /// * `query` - 查询条件
    ///
    /// # Returns
    ///
    /// 成功时返回分页结果。
    ///
    /// # Errors
    ///
    /// 当数据库查询失败时返回错误。
    async fn list_files(&self, query: FileQuery) -> Result<FilePage>;

    /// 检查文件是否存在
    ///
    /// # Arguments
    ///
    /// * `file_id` - 文件唯一标识
    ///
    /// # Returns
    ///
    /// 若文件存在且未归档则返回 `true`，否则返回 `false`。
    async fn exists(&self, file_id: &str) -> Result<bool>;

    /// 生成下载预签名 URL
    ///
    /// # Arguments
    ///
    /// * `file_id` - 文件唯一标识
    /// * `expires` - 签名有效期
    ///
    /// # Returns
    ///
    /// 成功时返回预签名下载 URL。
    ///
    /// # Errors
    ///
    /// * 当文件不存在或无存储路径时返回 `PresignError`
    /// * 当存储后端不支持预签名时返回 `UnsupportedError`
    async fn presign_download(&self, file_id: &str, expires: Duration) -> Result<String>;

    /// 生成上传预签名 URL
    ///
    /// # Arguments
    ///
    /// * `request` - 预签名上传请求
    /// * `expires` - 签名有效期
    ///
    /// # Returns
    ///
    /// 成功时返回预签名上传 URL 和创建的文件记录 ID。
    ///
    /// # Errors
    ///
    /// * 当存储后端不支持预签名写入时返回 `UnsupportedError`
    /// * 当数据库操作失败时返回错误
    async fn presign_upload(&self, request: PresignUploadRequest, expires: Duration) -> Result<PresignUploadResult>;

    /// 复制文件
    ///
    /// 支持跨平台复制，相同平台使用原生 copy，不同平台使用读取后写入。
    ///
    /// # Arguments
    ///
    /// * `file_id` - 源文件唯一标识
    /// * `target_platform` - 目标存储平台标识（若为 `None` 则使用默认平台）
    ///
    /// # Returns
    ///
    /// 成功时返回新文件的文件信息。
    ///
    /// # Errors
    ///
    /// * 当源文件不存在时返回 `NotFoundError`
    /// * 当复制操作失败时返回 `CopyError`
    async fn copy_file(&self, file_id: &str, target_platform: Option<&str>) -> Result<FileInfo>;

    /// 初始化分片上传
    ///
    /// # Arguments
    ///
    /// * `request` - 分片上传初始化请求
    ///
    /// # Returns
    ///
    /// 成功时返回分片上传会话信息。
    ///
    /// # Errors
    ///
    /// * 当存储后端不支持预签名写入时返回 `UnsupportedError`
    /// * 当数据库操作失败时返回错误
    async fn init_multipart_upload(&self, request: MultipartInitRequest) -> Result<MultipartSession>;

    /// 上传分片回调
    ///
    /// # Arguments
    ///
    /// * `session_id` - 分片上传会话 ID
    /// * `part` - 分片数据
    ///
    /// # Returns
    ///
    /// 成功时返回分片信息。
    ///
    /// # Errors
    ///
    /// * 当分片上传会话不存在时返回 `MultipartError`
    /// * 当数据库操作失败时返回错误
    async fn upload_part(&self, session_id: &str, part: PartData) -> Result<PartInfo>;

    /// 完成分片上传
    ///
    /// # Arguments
    ///
    /// * `session_id` - 分片上传会话 ID
    ///
    /// # Returns
    ///
    /// 成功时返回文件信息。
    ///
    /// # Errors
    ///
    /// * 当分片上传会话不存在时返回 `MultipartError`
    /// * 当数据库操作失败时返回错误
    async fn complete_multipart_upload(&self, session_id: &str) -> Result<FileInfo>;

    /// 取消分片上传
    ///
    /// 删除已上传的分片文件和数据库记录。
    ///
    /// # Arguments
    ///
    /// * `session_id` - 分片上传会话 ID
    ///
    /// # Returns
    ///
    /// 成功时返回 `Ok(())`。
    async fn abort_multipart_upload(&self, session_id: &str) -> Result<()>;
}

/// 默认存储服务实现
pub struct DefaultStorageService {
    /// 存储管理器
    manager: Arc<StorageManager>,
}

impl DefaultStorageService {
    /// 创建默认存储服务
    pub fn new(manager: Arc<StorageManager>) -> Self {
        Self { manager }
    }

    /// 计算 MD5 哈希
    fn compute_md5(data: &[u8]) -> String {
        let digest = md5::compute(data);
        format!("{:x}", digest)
    }

    /// 构建 Content-Disposition 头（RFC 5987）
    #[allow(dead_code)]
    fn build_content_disposition(filename: &str) -> String {
        let encoded = encode(filename);
        format!("attachment; filename=\"{}\"; filename*=UTF-8''{}", filename, encoded)
    }

    /// 获取数据库管理器和默认 db_id
    async fn get_db() -> Result<(&'static cmx_database::DatabaseManager, String)> {
        let mm = get_default_db_manager();
        let db_id = mm.get_default_db_id().await;
        Ok((mm, db_id))
    }

    /// 从 DataSet 中提取单个 FileDetail
    fn dataset_to_file_detail(dataset: &DataSet) -> Option<crate::types::FileDetail> {
        let row = dataset.iter().next()?;
        let schema = &dataset.schema;
        Some(crate::types::FileDetail {
            id: row.get_by_name_as(schema, "id").unwrap_or_default(),
            url: row.get_by_name_as(schema, "url").unwrap_or_default(),
            size: row.get_by_name_as(schema, "size"),
            filename: row.get_by_name_as(schema, "filename"),
            original_filename: row.get_by_name_as(schema, "original_filename"),
            base_path: row.get_by_name_as(schema, "base_path"),
            path: row.get_by_name_as(schema, "path"),
            ext: row.get_by_name_as(schema, "ext"),
            content_type: row.get_by_name_as(schema, "content_type"),
            platform: row.get_by_name_as(schema, "platform"),
            th_url: row.get_by_name_as(schema, "th_url"),
            th_path: row.get_by_name_as(schema, "th_path"),
            th_filename: row.get_by_name_as(schema, "th_filename"),
            th_size: row.get_by_name_as(schema, "th_size"),
            th_content_type: row.get_by_name_as(schema, "th_content_type"),
            object_id: row.get_by_name_as(schema, "object_id"),
            object_type: row.get_by_name_as(schema, "object_type"),
            metadata: row.get_by_name_as(schema, "metadata"),
            user_metadata: row.get_by_name_as(schema, "user_metadata"),
            th_metadata: row.get_by_name_as(schema, "th_metadata"),
            th_user_metadata: row.get_by_name_as(schema, "th_user_metadata"),
            attr: row.get_by_name_as(schema, "attr"),
            file_acl: row.get_by_name_as(schema, "file_acl"),
            th_file_acl: row.get_by_name_as(schema, "th_file_acl"),
            hash_info: row.get_by_name_as(schema, "hash_info"),
            upload_id: row.get_by_name_as(schema, "upload_id"),
            upload_status: row.get_by_name_as(schema, "upload_status"),
            archived: row.get_by_name_as(schema, "archived"),
            create_time: None,
            update_time: None,
            create_by: row.get_by_name_as(schema, "create_by"),
            create_name: row.get_by_name_as(schema, "create_name"),
            update_by: row.get_by_name_as(schema, "update_by"),
            update_name: row.get_by_name_as(schema, "update_name"),
        })
    }

    /// 根据主键查询文件详情
    async fn find_file_detail(file_id: &str) -> Result<crate::types::FileDetail> {
        let (mm, db_id) = Self::get_db().await?;
        let dataset = GenericCrudService::<FileDetailBmc>::get(
            mm, &db_id, None, Value::String(file_id.to_string()),
        )
            .await
            .map_err(Error::from)?;

        Self::dataset_to_file_detail(&dataset)
            .ok_or_else(|| Error::NotFoundError(format!("文件不存在: {}", file_id)))
    }

    /// 创建文件数据库记录
    async fn create_file_record(
        file_info: &FileInfo,
        _request: &UploadRequest,
        md5_hash: &str,
    ) -> Result<crate::types::FileDetail> {
        let (mm, db_id) = Self::get_db().await?;
        let data = FileDetailForCreate {
            id: Some(file_info.id.clone()),
            url: Some(file_info.url.clone()),
            size: Some(file_info.size),
            filename: Some(file_info.filename.clone()),
            original_filename: file_info.original_filename.clone(),
            base_path: file_info.base_path.clone(),
            path: file_info.path.clone(),
            ext: file_info.ext.clone(),
            content_type: file_info.content_type.clone(),
            platform: Some(file_info.platform.clone()),
            object_id: file_info.object_id.clone(),
            object_type: file_info.object_type.clone(),
            user_metadata: file_info.user_metadata.clone(),
            hash_info: Some(serde_json::json!({"md5": md5_hash}).to_string()),
            upload_status: Some(0),
            th_url: None,
            th_path: None,
            th_filename: None,
            th_size: None,
            th_content_type: None,
            metadata: None,
            th_metadata: None,
            th_user_metadata: None,
            attr: None,
            file_acl: None,
            th_file_acl: None,
            upload_id: None,
        };

        let dataset = GenericCrudService::<FileDetailBmc>::create(mm, &db_id, None, data)
            .await
            .map_err(Error::from)?;

        Self::dataset_to_file_detail(&dataset)
            .ok_or_else(|| Error::UploadError("创建文件数据库记录失败".to_string()))
    }

    /// 尝试秒传：根据 hash_info 和 platform 查找已有文件
    async fn try_instant_upload(hash_info: &str, platform: &str) -> Result<Option<FileInfo>> {
        let (mm, db_id) = Self::get_db().await?;
        let filter = FileDetailFilter {
            hash_info: Some(OpValsString(vec![OpValString::Eq(hash_info.to_string())])),
            platform: Some(OpValsString(vec![OpValString::Eq(platform.to_string())])),
            id: None,
            object_type: None,
            object_id: None,
            original_filename: None,
            upload_id: None,
            archived: None,
        };

        let list_options = ListOptions {
            limit: Some(1),
            offset: Some(0),
            order_bys: None,
        };

        let (dataset, _total) = GenericCrudService::<FileDetailBmc, FileDetailFilter>::page(
            mm, &db_id, None, Some(vec![filter]), list_options,
        )
            .await
            .map_err(Error::from)?;

        if let Some(detail) = Self::dataset_to_file_detail(&dataset)
            && detail.archived.unwrap_or(0) == 0
        {
            return Ok(Some(detail.to_file_info()));
        }
        Ok(None)
    }
}

#[async_trait]
impl StorageService for DefaultStorageService {
    async fn upload(&self, request: UploadRequest) -> Result<FileInfo> {
        let md5_hash = Self::compute_md5(&request.data);
        let hash_info = serde_json::json!({"md5": md5_hash}).to_string();

        let platform = request.platform.clone()
            .or_else(|| self.manager.get_default_platform().map(|s| s.to_string()))
            .unwrap_or_default();

        if let Some(existing) = Self::try_instant_upload(&hash_info, &platform).await? {
            info!(hash = %md5_hash, existing_id = %existing.id, "秒传命中");
            let mut file_info = existing.clone();
            file_info.id = uuid::Uuid::new_v4().to_string();
            file_info.original_filename = request.original_filename.clone();
            file_info.object_id = request.object_id.clone();
            file_info.object_type = request.object_type.clone();
            return Self::create_file_record(&file_info, &request, &md5_hash).await.map(|d| d.to_file_info());
        }

        let backend = self.manager.get_backend(Some(&platform))?;
        let config = self.manager.get_config(backend.platform())
            .ok_or_else(|| Error::ConfigError(format!("找不到平台配置: {}", backend.platform())))?;

        let content_type = detect_mime(
            &request.data,
            request.original_filename.as_deref(),
            request.content_type.as_deref(),
        );

        let ext = request.original_filename
            .as_deref()
            .map(extract_extension)
            .unwrap_or_default();

        let (storage_path, filename) = generate_storage_path(
            &config.base_path,
            request.object_type.as_deref(),
            &ext,
        );

        let write_opts = WriteOptions {
            content_type: Some(content_type.clone()),
            content_disposition: None,
            cache_control: None,
            user_metadata: request.user_metadata.clone(),
            acl: request.acl.clone(),
        };

        backend.write(&storage_path, request.data.clone(), write_opts).await?;

        let access_url = config.get_access_url(&storage_path);

        let file_info = FileInfo {
            id: uuid::Uuid::new_v4().to_string(),
            url: access_url,
            size: request.data.len() as i64,
            filename,
            original_filename: request.original_filename.clone(),
            base_path: Some(config.base_path.clone()),
            path: Some(storage_path.clone()),
            ext: Some(if ext.starts_with('.') { ext.clone() } else { format!(".{}", ext) }),
            content_type: Some(content_type),
            platform: backend.platform().to_string(),
            th_url: None,
            th_path: None,
            th_filename: None,
            th_size: None,
            th_content_type: None,
            object_id: request.object_id.clone(),
            object_type: request.object_type.clone(),
            user_metadata: request.user_metadata.as_ref().map(|m| {
                serde_json::to_string(m).unwrap_or_default()
            }),
            hash_info: Some(hash_info),
            upload_id: None,
            upload_status: Some(0),
            create_time: Some(chrono::Local::now().naive_utc()),
        };

        Self::create_file_record(&file_info, &request, &md5_hash).await?;

        info!(file_id = %file_info.id, platform = %file_info.platform, size = file_info.size, "文件上传成功");

        Ok(file_info)
    }

    async fn upload_with_thumbnail(&self, request: UploadRequest, thumbnail: ThumbnailData) -> Result<FileInfo> {
        let file_info = self.upload(request).await?;

        let platform = file_info.platform.clone();
        let backend = self.manager.get_backend(Some(&platform))?;
        let config = self.manager.get_config(&platform)
            .ok_or_else(|| Error::ConfigError(format!("找不到平台配置: {}", platform)))?;

        let ext = match thumbnail.content_type.as_str() {
            "image/png" => "png",
            "image/gif" => "gif",
            "image/webp" => "webp",
            _ => "jpg",
        };
        let th_filename = format!("thumb_{}.{}", file_info.id, ext);
        let base_path = config.base_path.trim_end_matches('/');
        let th_path = if base_path.is_empty() {
            format!("thumbnails/{}", th_filename)
        } else {
            format!("{}/thumbnails/{}", base_path, th_filename)
        };

        let write_opts = WriteOptions {
            content_type: Some(thumbnail.content_type.clone()),
            content_disposition: None,
            cache_control: None,
            user_metadata: None,
            acl: None,
        };
        backend.write(&th_path, thumbnail.data.clone(), write_opts).await?;

        let (mm, db_id) = Self::get_db().await?;
        let update_data = FileDetailForUpdate {
            th_url: Some(config.get_access_url(&th_path)),
            th_path: Some(th_path.clone()),
            th_filename: Some(th_filename.clone()),
            th_size: Some(thumbnail.data.len() as i64),
            th_content_type: Some(thumbnail.content_type.clone()),
            url: None,
            size: None,
            filename: None,
            upload_status: None,
            archived: None,
        };

        GenericCrudService::<FileDetailBmc>::update(
            mm, &db_id, None, Value::String(file_info.id.clone()), update_data,
        )
            .await
            .map_err(Error::from)?;

        let mut result = file_info;
        result.th_url = Some(config.get_access_url(&th_path));
        result.th_path = Some(th_path);
        result.th_filename = Some(th_filename);
        result.th_size = Some(thumbnail.data.len() as i64);
        result.th_content_type = Some(thumbnail.content_type);

        Ok(result)
    }

    async fn download(&self, file_id: &str) -> Result<FileDownload> {
        let detail = Self::find_file_detail(file_id).await?;
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

    async fn download_thumbnail(&self, file_id: &str) -> Result<FileDownload> {
        let detail = Self::find_file_detail(file_id).await?;

        let th_path = detail.th_path.as_deref()
            .ok_or_else(|| Error::NotFoundError(format!("缩略图路径不存在: {}", file_id)))?;

        let backend = self.manager.get_backend(detail.platform.as_deref())?;
        let data = backend.read(th_path).await?;

        let content_type = detail.th_content_type.clone().unwrap_or_else(|| "image/png".to_string());
        let file_info = detail.to_file_info();

        Ok(FileDownload {
            data,
            file_info,
            content_type,
            content_disposition: format!("inline; filename=\"thumbnail_{}\"", file_id),
            content_length: detail.th_size.unwrap_or(0) as u64,
        })
    }

    async fn delete(&self, file_id: &str) -> Result<()> {
        let detail = Self::find_file_detail(file_id).await?;

        if let Some(path) = detail.path.as_deref() {
            let platform = detail.platform.as_deref().unwrap_or("");
            if let Ok(backend) = self.manager.get_backend(Some(platform)) {
                let _ = backend.delete(path).await;
            }
        }

        let (mm, db_id) = Self::get_db().await?;
        let update_data = FileDetailForUpdate {
            archived: Some(1),
            url: None,
            size: None,
            filename: None,
            upload_status: None,
            th_url: None,
            th_path: None,
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

    async fn batch_delete(&self, file_ids: &[String]) -> Result<Vec<Result<()>>> {
        let mut results = Vec::new();
        for file_id in file_ids {
            results.push(self.delete(file_id).await);
        }
        Ok(results)
    }

    async fn get_file_info(&self, file_id: &str) -> Result<FileInfo> {
        let detail = Self::find_file_detail(file_id).await?;
        Ok(detail.to_file_info())
    }

    async fn list_files(&self, query: FileQuery) -> Result<FilePage> {
        let (mm, db_id) = Self::get_db().await?;

        let mut filter = FileDetailFilter {
            id: None,
            platform: None,
            object_type: None,
            object_id: None,
            original_filename: None,
            upload_id: None,
            hash_info: None,
            archived: None,
        };
        filter.with_active_only();

        if let Some(ref ot) = query.object_type {
            filter.object_type = Some(OpValsString(vec![OpValString::Eq(ot.clone())]));
        }
        if let Some(ref oid) = query.object_id {
            filter.object_id = Some(OpValsString(vec![OpValString::Eq(oid.clone())]));
        }
        if let Some(ref p) = query.platform {
            filter.platform = Some(OpValsString(vec![OpValString::Eq(p.clone())]));
        }
        if let Some(ref fn_) = query.original_filename {
            filter.original_filename = Some(OpValsString(vec![OpValString::Contains(fn_.clone())]));
        }

        let page = query.page.unwrap_or(1);
        let page_size = query.page_size.unwrap_or(10);

        let list_options = ListOptions {
            limit: Some(page_size as i64),
            offset: Some(((page - 1) * page_size) as i64),
            order_bys: Some("create_time desc".into()),
        };

        let (dataset, total) = GenericCrudService::<FileDetailBmc, FileDetailFilter>::page(
            mm, &db_id, None, Some(vec![filter]), list_options,
        )
            .await
            .map_err(Error::from)?;

        let items: Vec<FileInfo> = dataset
            .iter()
            .map(|row| {
                let schema = &dataset.schema;
                FileInfo {
                    id: row.get_by_name_as(schema, "id").unwrap_or_default(),
                    url: row.get_by_name_as(schema, "url").unwrap_or_default(),
                    size: row.get_by_name_as(schema, "size").unwrap_or(0),
                    filename: row.get_by_name_as(schema, "filename").unwrap_or_default(),
                    original_filename: row.get_by_name_as(schema, "original_filename"),
                    base_path: row.get_by_name_as(schema, "base_path"),
                    path: row.get_by_name_as(schema, "path"),
                    ext: row.get_by_name_as(schema, "ext"),
                    content_type: row.get_by_name_as(schema, "content_type"),
                    platform: row.get_by_name_as(schema, "platform").unwrap_or_default(),
                    th_url: row.get_by_name_as(schema, "th_url"),
                    th_path: row.get_by_name_as(schema, "th_path"),
                    th_filename: row.get_by_name_as(schema, "th_filename"),
                    th_size: row.get_by_name_as(schema, "th_size"),
                    th_content_type: row.get_by_name_as(schema, "th_content_type"),
                    object_id: row.get_by_name_as(schema, "object_id"),
                    object_type: row.get_by_name_as(schema, "object_type"),
                    user_metadata: row.get_by_name_as(schema, "user_metadata"),
                    hash_info: row.get_by_name_as(schema, "hash_info"),
                    upload_id: row.get_by_name_as(schema, "upload_id"),
                    upload_status: row.get_by_name_as(schema, "upload_status"),
                    create_time: None,
                }
            })
            .collect();

        Ok(FilePage {
            total: total as u64,
            page,
            page_size,
            items,
        })
    }

    async fn exists(&self, file_id: &str) -> Result<bool> {
        let (mm, db_id) = Self::get_db().await?;
        let result = GenericCrudService::<FileDetailBmc>::get(
            mm, &db_id, None, Value::String(file_id.to_string()),
        )
            .await;

        match result {
            Ok(dataset) => Ok(dataset.iter().next().is_some()),
            Err(_) => Ok(false),
        }
    }

    async fn presign_download(&self, file_id: &str, expires: Duration) -> Result<String> {
        let detail = Self::find_file_detail(file_id).await?;

        let path = detail.path.as_deref()
            .ok_or_else(|| Error::PresignError(format!("文件 {} 无存储路径", file_id)))?;

        let backend = self.manager.get_backend(detail.platform.as_deref())?;
        let url = backend.presign_read(path, expires).await?;

        info!(file_id = file_id, expires_secs = expires.as_secs(), "生成下载预签名 URL");
        Ok(url)
    }

    async fn presign_upload(&self, request: PresignUploadRequest, expires: Duration) -> Result<PresignUploadResult> {
        let platform = request.platform.clone();
        let backend = self.manager.get_backend(platform.as_deref())?;
        let config = self.manager.get_config(backend.platform())
            .ok_or_else(|| Error::ConfigError(format!("找不到平台配置: {}", backend.platform())))?;

        let ext = extract_extension(&request.filename);
        let (storage_path, _filename) = generate_storage_path(&config.base_path, None, &ext);

        let url = backend.presign_write(&storage_path, expires).await?;

        let file_id = uuid::Uuid::new_v4().to_string();
        let access_url = config.get_access_url(&storage_path);

        let (mm, db_id) = Self::get_db().await?;
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

    async fn copy_file(&self, file_id: &str, target_platform: Option<&str>) -> Result<FileInfo> {
        let detail = Self::find_file_detail(file_id).await?;

        let src_path = detail.path.as_deref()
            .ok_or_else(|| Error::CopyError(format!("源文件 {} 无存储路径", file_id)))?;

        let src_platform = detail.platform.as_deref().unwrap_or("");
        let src_backend = self.manager.get_backend(Some(src_platform))?;

        let target_backend = self.manager.get_backend(target_platform)?;
        let target_config = self.manager.get_config(target_backend.platform())
            .ok_or_else(|| Error::ConfigError(format!("找不到目标平台配置: {}", target_backend.platform())))?;

        let ext = detail.ext.as_deref().unwrap_or("");
        let (target_path, filename) = generate_storage_path(
            &target_config.base_path,
            detail.object_type.as_deref(),
            ext,
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

        let (mm, db_id) = Self::get_db().await?;
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

    async fn init_multipart_upload(&self, request: MultipartInitRequest) -> Result<MultipartSession> {
        let backend = self.manager.get_backend(request.platform.as_deref())?;
        let caps = backend.capabilities();

        if !caps.presign_write {
            return Err(Error::UnsupportedError(
                format!("存储平台 {} 不支持预签名上传（分片上传必需）", backend.platform())
            ));
        }

        let upload_id = uuid::Uuid::new_v4().to_string();
        let file_id = uuid::Uuid::new_v4().to_string();

        let ext = extract_extension(&request.filename);
        let config = self.manager.get_config(backend.platform())
            .ok_or_else(|| Error::ConfigError(format!("找不到平台配置: {}", backend.platform())))?;

        let (storage_path, _) = generate_storage_path(
            &config.base_path,
            request.object_type.as_deref(),
            &ext,
        );

        let access_url = config.get_access_url(&storage_path);

        let (mm, db_id) = Self::get_db().await?;
        let data = FileDetailForCreate {
            id: Some(file_id.clone()),
            url: Some(access_url),
            path: Some(storage_path.clone()),
            platform: Some(backend.platform().to_string()),
            original_filename: Some(request.filename.clone()),
            content_type: request.content_type.clone(),
            ext: Some(if ext.starts_with('.') { ext.clone() } else { format!(".{}", ext) }),
            upload_id: Some(upload_id.clone()),
            upload_status: Some(1),
            object_id: request.object_id.clone(),
            object_type: request.object_type.clone(),
            ..Default::default()
        };

        GenericCrudService::<FileDetailBmc>::create(mm, &db_id, None, data)
            .await
            .map_err(Error::from)?;

        let mut presigned_urls = Vec::new();
        for part_num in 1..=request.total_parts {
            let part_path = format!("{}.part.{}", storage_path, part_num);
            let url = backend.presign_write(&part_path, Duration::from_secs(3600)).await?;
            presigned_urls.push(PresignedPartUrl {
                part_number: part_num,
                upload_url: url,
            });
        }

        info!(upload_id = %upload_id, file_id = %file_id, total_parts = request.total_parts, "分片上传会话已初始化");

        Ok(MultipartSession {
            upload_id,
            file_id,
            presigned_urls,
            total_parts: request.total_parts,
        })
    }

    async fn upload_part(&self, session_id: &str, part: PartData) -> Result<PartInfo> {
        let (mm, db_id) = Self::get_db().await?;

        let filter = FileDetailFilter {
            upload_id: Some(OpValsString(vec![OpValString::Eq(session_id.to_string())])),
            ..Default::default()
        };

        let list_options = ListOptions {
            limit: Some(1),
            offset: Some(0),
            order_bys: None,
        };

        let (dataset, _) = GenericCrudService::<FileDetailBmc, FileDetailFilter>::page(
            mm, &db_id, None, Some(vec![filter]), list_options,
        )
            .await
            .map_err(Error::from)?;

        let detail = Self::dataset_to_file_detail(&dataset)
            .ok_or_else(|| Error::MultipartError(format!("分片上传会话不存在: {}", session_id)))?;

        let part_id = uuid::Uuid::new_v4().to_string();
        let part_data = FilePartDetailForCreate {
            id: Some(part_id),
            platform: detail.platform.clone(),
            upload_id: Some(session_id.to_string()),
            e_tag: Some(part.e_tag.clone()),
            part_number: Some(part.part_number as i32),
            part_size: Some(part.part_size),
            hash_info: None,
        };

        GenericCrudService::<FilePartDetailBmc>::create(mm, &db_id, None, part_data)
            .await
            .map_err(Error::from)?;

        info!(upload_id = session_id, part_number = part.part_number, part_size = part.part_size, "分片上传完成");

        Ok(PartInfo {
            part_number: part.part_number,
            e_tag: part.e_tag,
            part_size: part.part_size,
        })
    }

    async fn complete_multipart_upload(&self, session_id: &str) -> Result<FileInfo> {
        let (mm, db_id) = Self::get_db().await?;

        let filter = FileDetailFilter {
            upload_id: Some(OpValsString(vec![OpValString::Eq(session_id.to_string())])),
            ..Default::default()
        };

        let list_options = ListOptions {
            limit: Some(1),
            offset: Some(0),
            order_bys: None,
        };

        let (dataset, _) = GenericCrudService::<FileDetailBmc, FileDetailFilter>::page(
            mm, &db_id, None, Some(vec![filter]), list_options,
        )
            .await
            .map_err(Error::from)?;

        let detail = Self::dataset_to_file_detail(&dataset)
            .ok_or_else(|| Error::MultipartError(format!("分片上传会话不存在: {}", session_id)))?;

        let update_data = FileDetailForUpdate {
            upload_status: Some(2),
            url: None,
            size: None,
            filename: None,
            th_url: None,
            th_path: None,
            th_filename: None,
            th_size: None,
            th_content_type: None,
            archived: None,
        };

        GenericCrudService::<FileDetailBmc>::update(
            mm, &db_id, None, Value::String(detail.id.clone()), update_data,
        )
            .await
            .map_err(Error::from)?;

        info!(upload_id = session_id, file_id = %detail.id, "分片上传完成");
        Ok(detail.to_file_info())
    }

    async fn abort_multipart_upload(&self, session_id: &str) -> Result<()> {
        let (mm, db_id) = Self::get_db().await?;

        let filter = FileDetailFilter {
            upload_id: Some(OpValsString(vec![OpValString::Eq(session_id.to_string())])),
            ..Default::default()
        };

        let list_options = ListOptions {
            limit: Some(1),
            offset: Some(0),
            order_bys: None,
        };

        let (dataset, _) = GenericCrudService::<FileDetailBmc, FileDetailFilter>::page(
            mm, &db_id, None, Some(vec![filter]), list_options,
        )
            .await
            .map_err(Error::from)?;

        if let Some(detail) = Self::dataset_to_file_detail(&dataset) {
            if let Some(path) = detail.path.as_deref() {
                let platform = detail.platform.as_deref().unwrap_or("");
                if let Ok(backend) = self.manager.get_backend(Some(platform)) {
                    for i in 1..=10000u32 {
                        let part_path = format!("{}.part.{}", path, i);
                        if backend.delete(&part_path).await.is_err() {
                            break;
                        }
                    }
                }
            }

            let file_id = detail.id.clone();

            let filter_parts = FileDetailFilter {
                upload_id: Some(OpValsString(vec![OpValString::Eq(session_id.to_string())])),
                ..Default::default()
            };
            let list_opts_parts = ListOptions {
                limit: Some(1000),
                offset: Some(0),
                order_bys: None,
            };
            let (parts_dataset, _) = GenericCrudService::<FilePartDetailBmc, FileDetailFilter>::page(
                mm, &db_id, None, Some(vec![filter_parts]), list_opts_parts,
            )
                .await
                .map_err(Error::from)?;
            for row in parts_dataset.iter() {
                let schema = &parts_dataset.schema;
                if let Some(part_id) = row.get_by_name_as::<String>(schema, "id") {
                    let _ = GenericCrudService::<FilePartDetailBmc>::delete(
                        mm, &db_id, None, vec![Value::String(part_id)],
                    )
                        .await;
                }
            }

            let _ = GenericCrudService::<FileDetailBmc>::delete(
                mm, &db_id, None, vec![Value::String(file_id)],
            )
                .await;
        }

        info!(upload_id = session_id, "分片上传已取消");
        Ok(())
    }
}
