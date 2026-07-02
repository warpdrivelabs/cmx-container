//! 分片上传
//!
//! 实现 [`StorageService`] 的分片上传相关方法：初始化、上传分片回调、完成、取消。

use std::time::Duration;

use cmx_database::crud::GenericCrudService;
use modql::filter::{ListOptions, OpValString, OpValsString};
use serde_json::Value;
use tracing::info;

use crate::bmc::*;
use crate::error::{Error, Result};
use crate::path_gen::{extract_extension, generate_storage_path};
use crate::service::DefaultStorageService;
use crate::types::*;

impl DefaultStorageService {
    /// 初始化分片上传（[`crate::service::StorageService::init_multipart_upload`] 的实现）。
    pub(super) async fn init_multipart_upload(
        &self,
        request: MultipartInitRequest,
    ) -> Result<MultipartSession> {
        let backend = self.manager.get_backend(request.platform.as_deref())?;
        let caps = backend.capabilities();

        if !caps.presign_write {
            return Err(Error::UnsupportedError(format!(
                "存储平台 {} 不支持预签名上传（分片上传必需）",
                backend.platform()
            )));
        }

        let upload_id = uuid::Uuid::new_v4().to_string();
        let file_id = uuid::Uuid::new_v4().to_string();

        let ext = extract_extension(&request.filename);
        let config = self
            .manager
            .get_config(backend.platform())
            .ok_or_else(|| Error::ConfigError(format!("找不到平台配置: {}", backend.platform())))?;

        let (storage_path, _) = generate_storage_path(
            &config.base_path,
            request.object_type.as_deref(),
            &ext,
            &config.storage_type,
        );

        let access_url = config.get_access_url(&storage_path);

        let (mm, db_id) = self.get_db().await?;
        let data = FileDetailForCreate {
            id: Some(file_id.clone()),
            url: Some(access_url),
            path: Some(storage_path.clone()),
            platform: Some(backend.platform().to_string()),
            original_filename: Some(request.filename.clone()),
            content_type: request.content_type.clone(),
            ext: Some(if ext.starts_with('.') {
                ext.clone()
            } else {
                format!(".{}", ext)
            }),
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
            let url = backend
                .presign_write(&part_path, Duration::from_secs(3600))
                .await?;
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

    /// 上传分片回调（[`crate::service::StorageService::upload_part`] 的实现）。
    pub(super) async fn upload_part(&self, session_id: &str, part: PartData) -> Result<PartInfo> {
        let (mm, db_id) = self.get_db().await?;

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
            mm,
            &db_id,
            None,
            Some(vec![filter]),
            list_options,
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

        info!(
            upload_id = session_id,
            part_number = part.part_number,
            part_size = part.part_size,
            "分片上传完成"
        );

        Ok(PartInfo {
            part_number: part.part_number,
            e_tag: part.e_tag,
            part_size: part.part_size,
        })
    }

    /// 完成分片上传（[`crate::service::StorageService::complete_multipart_upload`] 的实现）。
    pub(super) async fn complete_multipart_upload(&self, session_id: &str) -> Result<FileInfo> {
        let (mm, db_id) = self.get_db().await?;

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
            mm,
            &db_id,
            None,
            Some(vec![filter]),
            list_options,
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
            th_filename: None,
            th_size: None,
            th_content_type: None,
            archived: None,
        };

        GenericCrudService::<FileDetailBmc>::update(
            mm,
            &db_id,
            None,
            Value::String(detail.id.clone()),
            update_data,
        )
        .await
        .map_err(Error::from)?;

        info!(upload_id = session_id, file_id = %detail.id, "分片上传完成");
        Ok(detail.to_file_info())
    }

    /// 取消分片上传（[`crate::service::StorageService::abort_multipart_upload`] 的实现）。
    pub(super) async fn abort_multipart_upload(&self, session_id: &str) -> Result<()> {
        let (mm, db_id) = self.get_db().await?;

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
            mm,
            &db_id,
            None,
            Some(vec![filter]),
            list_options,
        )
        .await
        .map_err(Error::from)?;

        if let Some(detail) = Self::dataset_to_file_detail(&dataset) {
            let file_id = detail.id.clone();

            let filter_parts = FilePartDetailFilter {
                upload_id: Some(OpValsString(vec![OpValString::Eq(session_id.to_string())])),
                ..Default::default()
            };
            let list_opts_parts = ListOptions {
                limit: Some(10000),
                offset: Some(0),
                order_bys: None,
            };
            let (parts_dataset, _) =
                GenericCrudService::<FilePartDetailBmc, FilePartDetailFilter>::page(
                    mm,
                    &db_id,
                    None,
                    Some(vec![filter_parts]),
                    list_opts_parts,
                )
                .await
                .map_err(Error::from)?;

            if let Some(path) = detail.path.as_deref() {
                let platform = detail.platform.as_deref().unwrap_or("");
                if let Ok(backend) = self.manager.get_backend(Some(platform)) {
                    for row in parts_dataset.iter() {
                        let schema = &parts_dataset.schema;
                        if let Some(part_number) = row.get_by_name_as::<i32>(schema, "part_number")
                        {
                            let part_path = format!("{}.part.{}", path, part_number);
                            let _ = backend.delete(&part_path).await;
                        }
                    }
                }
            }

            for row in parts_dataset.iter() {
                let schema = &parts_dataset.schema;
                if let Some(part_id) = row.get_by_name_as::<String>(schema, "id") {
                    let _ = GenericCrudService::<FilePartDetailBmc>::delete(
                        mm,
                        &db_id,
                        None,
                        vec![Value::String(part_id)],
                    )
                    .await;
                }
            }

            let _ = GenericCrudService::<FileDetailBmc>::delete(
                mm,
                &db_id,
                None,
                vec![Value::String(file_id)],
            )
            .await;
        }

        info!(upload_id = session_id, "分片上传已取消");
        Ok(())
    }
}
