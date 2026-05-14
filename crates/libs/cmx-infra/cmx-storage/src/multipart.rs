//! 手动分片上传管理模块
//!
//! 提供大文件分片上传的完整生命周期管理。
//!
//! ## 架构说明
//!
//! 本模块的 `MultipartManager` 目前作为独立工具存在，
//! 分片逻辑已在 `StorageService` 中直接实现。
//! `MultipartManager` 可在以下场景中使用：
//! - 需要在非 HTTP handler 上下文中发起分片上传
//! - 需要精细控制分片上传的各个步骤
//! - 将来作为独立服务使用

use std::sync::Arc;
use std::time::Duration;

use cmx_database::crud::GenericCrudService;
use cmx_database::get_default_db_manager;
use modql::filter::{ListOptions, OpValString, OpValsString};
use serde_json::Value;
use tracing::info;
use uuid::Uuid;

use crate::bmc::*;
use crate::error::{Error, Result};
use crate::manager::StorageManager;
use crate::path_gen::{extract_extension, generate_storage_path};
use crate::types::*;

/// 分片上传管理器
///
/// 提供大文件分片上传的高级管理功能，包括初始化、上传、记录和完成等操作。
///
/// # Examples
///
/// ```ignore
/// let manager = StorageManager::new(&config)?;
/// let multipart = MultipartManager::new(Arc::new(manager));
/// let session = multipart.init_upload(request).await?;
/// ```
pub struct MultipartManager {
    /// 存储管理器引用
    manager: Arc<StorageManager>,
}

impl MultipartManager {
    /// 创建分片上传管理器
    ///
    /// # Arguments
    ///
    /// * `manager` - 存储管理器实例
    ///
    /// # Returns
    ///
    /// 返回新的 `MultipartManager` 实例。
    pub fn new(manager: Arc<StorageManager>) -> Self {
        Self { manager }
    }

    /// 获取数据库管理器和默认 db_id
    async fn get_db() -> Result<(&'static cmx_database::DatabaseManager, String)> {
        let mm = get_default_db_manager();
        let db_id = mm.get_default_db_id().await;
        Ok((mm, db_id))
    }

    /// 初始化分片上传会话
    ///
    /// 创建 upload_id，生成每个分片的预签名上传 URL，
    /// 并在数据库中创建文件记录（upload_status=1）。
    ///
    /// # Arguments
    ///
    /// * `request` - 分片上传初始化请求
    ///
    /// # Returns
    ///
    /// 成功时返回分片上传会话信息，包含所有分片的预签名 URL。
    ///
    /// # Errors
    ///
    /// * 当存储平台不支持预签名写入时返回 `UnsupportedError`
    /// * 当平台配置不存在时返回 `ConfigError`
    /// * 当数据库操作失败时返回相应的错误
    pub async fn init_upload(&self, request: MultipartInitRequest) -> Result<MultipartSession> {
        let backend = self.manager.get_backend(request.platform.as_deref())?;
        let caps = backend.capabilities();

        if !caps.presign_write {
            return Err(Error::UnsupportedError(
                format!("存储平台 {} 不支持预签名上传（分片上传必需）", backend.platform())
            ));
        }

        let upload_id = Uuid::new_v4().to_string();
        let file_id = Uuid::new_v4().to_string();

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

    /// 记录分片上传完成
    ///
    /// 在数据库中创建分片记录。
    ///
    /// # Arguments
    ///
    /// * `part` - 分片数据（包含 ETag 和大小信息）
    ///
    /// # Returns
    ///
    /// 成功时返回分片信息。
    ///
    /// # Errors
    ///
    /// * 当分片上传会话不存在时返回 `MultipartError`
    /// * 当数据库操作失败时返回相应的错误
    pub async fn record_part(&self, part: &PartData) -> Result<PartInfo> {
        let (mm, db_id) = Self::get_db().await?;

        let filter = FileDetailFilter {
            upload_id: Some(OpValsString(vec![OpValString::Eq(part.upload_id.clone())])),
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

        let row = dataset.iter().next()
            .ok_or_else(|| Error::MultipartError(format!("分片上传会话不存在: {}", part.upload_id)))?;
        let platform: Option<String> = row.get_by_name_as(&dataset.schema, "platform");

        let part_id = Uuid::new_v4().to_string();
        let part_data = FilePartDetailForCreate {
            id: Some(part_id),
            platform,
            upload_id: Some(part.upload_id.clone()),
            e_tag: Some(part.e_tag.clone()),
            part_number: Some(part.part_number as i32),
            part_size: Some(part.part_size),
            hash_info: None,
        };

        GenericCrudService::<FilePartDetailBmc>::create(mm, &db_id, None, part_data)
            .await
            .map_err(Error::from)?;

        info!(upload_id = %part.upload_id, part_number = part.part_number, part_size = part.part_size, "分片上传完成");

        Ok(PartInfo {
            part_number: part.part_number,
            e_tag: part.e_tag.clone(),
            part_size: part.part_size,
        })
    }

    /// 完成分片上传
    ///
    /// 将 FileDetail 的 upload_status 更新为 2（上传完成）。
    ///
    /// # Arguments
    ///
    /// * `upload_id` - 分片上传会话 ID
    ///
    /// # Returns
    ///
    /// 成功时返回文件信息。
    ///
    /// # Errors
    ///
    /// * 当分片上传会话不存在时返回 `MultipartError`
    /// * 当数据库操作失败时返回相应的错误
    pub async fn complete_upload(&self, upload_id: &str) -> Result<FileInfo> {
        let (mm, db_id) = Self::get_db().await?;

        let filter = FileDetailFilter {
            upload_id: Some(OpValsString(vec![OpValString::Eq(upload_id.to_string())])),
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

        let row = dataset.iter().next()
            .ok_or_else(|| Error::MultipartError(format!("分片上传会话不存在: {}", upload_id)))?;
        let schema = &dataset.schema;
        let file_id: String = row.get_by_name_as(schema, "id").unwrap_or_default();

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

        let updated_dataset = GenericCrudService::<FileDetailBmc>::update(
            mm, &db_id, None, Value::String(file_id.clone()), update_data,
        )
            .await
            .map_err(Error::from)?;

        let updated_row = updated_dataset.iter().next()
            .ok_or_else(|| Error::MultipartError("完成分片上传后查询记录失败".to_string()))?;
        let updated_schema = &updated_dataset.schema;

        let file_info = FileInfo {
            id: updated_row.get_by_name_as(updated_schema, "id").unwrap_or_default(),
            url: updated_row.get_by_name_as(updated_schema, "url").unwrap_or_default(),
            size: updated_row.get_by_name_as(updated_schema, "size").unwrap_or(0),
            filename: updated_row.get_by_name_as(updated_schema, "filename").unwrap_or_default(),
            original_filename: updated_row.get_by_name_as(updated_schema, "original_filename"),
            base_path: updated_row.get_by_name_as(updated_schema, "base_path"),
            path: updated_row.get_by_name_as(updated_schema, "path"),
            ext: updated_row.get_by_name_as(updated_schema, "ext"),
            content_type: updated_row.get_by_name_as(updated_schema, "content_type"),
            platform: updated_row.get_by_name_as(updated_schema, "platform").unwrap_or_default(),
            th_url: updated_row.get_by_name_as(updated_schema, "th_url"),
            th_path: updated_row.get_by_name_as(updated_schema, "th_path"),
            th_filename: updated_row.get_by_name_as(updated_schema, "th_filename"),
            th_size: updated_row.get_by_name_as(updated_schema, "th_size"),
            th_content_type: updated_row.get_by_name_as(updated_schema, "th_content_type"),
            object_id: updated_row.get_by_name_as(updated_schema, "object_id"),
            object_type: updated_row.get_by_name_as(updated_schema, "object_type"),
            user_metadata: updated_row.get_by_name_as(updated_schema, "user_metadata"),
            hash_info: updated_row.get_by_name_as(updated_schema, "hash_info"),
            upload_id: updated_row.get_by_name_as(updated_schema, "upload_id"),
            upload_status: updated_row.get_by_name_as(updated_schema, "upload_status"),
            create_time: None,
        };

        info!(upload_id = upload_id, file_id = %file_info.id, "分片上传完成");
        Ok(file_info)
    }

    /// 取消分片上传
    ///
    /// 删除 FilePartDetail 和 FileDetail 数据库记录。
    ///
    /// # Arguments
    ///
    /// * `upload_id` - 分片上传会话 ID
    ///
    /// # Returns
    ///
    /// 成功时返回 `Ok(())`。
    pub async fn abort_upload(&self, upload_id: &str) -> Result<()> {
        let (mm, db_id) = Self::get_db().await?;

        let filter = FileDetailFilter {
            upload_id: Some(OpValsString(vec![OpValString::Eq(upload_id.to_string())])),
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

        if let Some(row) = dataset.iter().next() {
            let file_id: String = row.get_by_name_as(&dataset.schema, "id").unwrap_or_default();
            let _ = GenericCrudService::<FileDetailBmc>::delete(
                mm, &db_id, None, vec![Value::String(file_id)],
            )
                .await;
        }

        info!(upload_id = upload_id, "分片上传已取消");
        Ok(())
    }
}
