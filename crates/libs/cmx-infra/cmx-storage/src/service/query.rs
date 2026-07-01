//! 文件查询
//!
//! 实现 [`StorageService::get_file_info`]、[`StorageService::list_files`] 与 [`StorageService::exists`]。

use cmx_database::crud::GenericCrudService;
use modql::filter::{ListOptions, OpValString, OpValsString};
use serde_json::Value;

use crate::bmc::*;
use crate::error::{Error, Result};
use crate::service::DefaultStorageService;
use crate::types::*;

impl DefaultStorageService {
    /// 获取文件信息（[`crate::service::StorageService::get_file_info`] 的实现）。
    pub(super) async fn get_file_info(&self, file_id: &str) -> Result<FileInfo> {
        let detail = self.find_file_detail(file_id).await?;
        Ok(detail.to_file_info())
    }

    /// 分页查询文件列表（[`crate::service::StorageService::list_files`] 的实现）。
    pub(super) async fn list_files(&self, query: FileQuery) -> Result<FilePage> {
        let (mm, db_id) = self.get_db().await?;

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

        let items: Vec<FileInfo> = Self::dataset_to_file_details(&dataset)
            .into_iter()
            .map(|d| d.to_file_info())
            .collect();

        Ok(FilePage {
            total: total as u64,
            page,
            page_size,
            items,
        })
    }

    /// 检查文件是否存在（[`crate::service::StorageService::exists`] 的实现）。
    pub(super) async fn exists(&self, file_id: &str) -> Result<bool> {
        let (mm, db_id) = self.get_db().await?;
        let result = GenericCrudService::<FileDetailBmc>::get(
            mm, &db_id, None, Value::String(file_id.to_string()),
        )
            .await;

        match result {
            Ok(dataset) => Ok(dataset.iter().next().is_some()),
            Err(_) => Ok(false),
        }
    }
}
