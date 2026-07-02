//! 存储服务内部辅助方法
//!
//! 包含 `DefaultStorageService` 的内部工具方法：MD5 哈希计算、Content-Disposition 头构建、
//! 数据库访问辅助、`DataSet` 到 `FileDetail` 的映射、文件记录的查询/创建/秒传检测等。
//!
//! 这些方法以 `pub(super)` 暴露给 [`super`] 的其他子模块使用，不构成对外 API。

use cmx_core::model::data::dataset::DataSet;
use cmx_database::crud::GenericCrudService;
use modql::filter::{ListOptions, OpValString, OpValsString};
use serde_json::Value;
use urlencoding::encode;

use crate::bmc::*;
use crate::error::{Error, Result};
use crate::service::DefaultStorageService;
use crate::types::*;

impl DefaultStorageService {
    /// 计算给定数据的 MD5 哈希值。
    ///
    /// # Arguments
    ///
    /// * `data` - 待计算哈希的二进制数据。
    ///
    /// # Returns
    ///
    /// 返回小写十六进制格式的 MD5 哈希字符串。
    pub(super) fn compute_md5(data: &[u8]) -> String {
        let digest = md5::compute(data);
        format!("{:x}", digest)
    }

    /// 构建 Content-Disposition 响应头（RFC 5987）。
    ///
    /// 同时提供 `filename` 和 `filename*` 参数，兼容旧客户端和支持 Unicode 的客户端。
    ///
    /// # Arguments
    ///
    /// * `filename` - 原始文件名。
    ///
    /// # Returns
    ///
    /// 符合 RFC 5987 规范的 Content-Disposition 头字符串。
    #[allow(dead_code)]
    pub(super) fn build_content_disposition(filename: &str) -> String {
        let encoded = encode(filename);
        format!(
            "attachment; filename=\"{}\"; filename*=UTF-8''{}",
            filename, encoded
        )
    }

    pub(super) async fn get_db(&self) -> Result<(&'static cmx_database::DatabaseManager, String)> {
        let db_id = self.db.get_default_db_id().await;
        Ok((self.db, db_id))
    }

    /// 从 `DataSet` 中提取单个 `FileDetail` 记录。
    ///
    /// 取 `DataSet` 的第一行数据，按列名映射为 `FileDetail` 结构体。
    ///
    /// # Arguments
    ///
    /// * `dataset` - 数据库查询返回的数据集。
    ///
    /// # Returns
    ///
    /// 数据集非空时返回 `Some(FileDetail)`，否则返回 `None`。
    pub(super) fn dataset_to_file_detail(dataset: &DataSet) -> Option<crate::bmc::FileDetail> {
        let row = dataset.iter().next()?;
        let schema = &dataset.schema;
        Some(Self::row_to_file_detail(row, schema))
    }

    pub(super) fn dataset_to_file_details(dataset: &DataSet) -> Vec<crate::bmc::FileDetail> {
        let schema = &dataset.schema;
        dataset
            .iter()
            .map(|row| Self::row_to_file_detail(row, schema))
            .collect()
    }

    fn row_to_file_detail(
        row: &cmx_core::model::data::dataset::rds::Row,
        schema: &cmx_core::model::data::dataset::Schema,
    ) -> crate::bmc::FileDetail {
        crate::bmc::FileDetail {
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
            create_time: row
                .get_by_name_as::<chrono::DateTime<chrono::Utc>>(schema, "create_time")
                .map(|dt| dt.naive_utc()),
            update_time: row
                .get_by_name_as::<chrono::DateTime<chrono::Utc>>(schema, "update_time")
                .map(|dt| dt.naive_utc()),
            create_by: row.get_by_name_as(schema, "create_by"),
            create_name: row.get_by_name_as(schema, "create_name"),
            update_by: row.get_by_name_as(schema, "update_by"),
            update_name: row.get_by_name_as(schema, "update_name"),
        }
    }

    /// 根据主键查询文件详情。
    ///
    /// # Arguments
    ///
    /// * `file_id` - 文件唯一标识。
    ///
    /// # Returns
    ///
    /// 成功时返回 `FileDetail`。
    ///
    /// # Errors
    ///
    /// 当文件不存在时返回 `NotFoundError`。
    pub(super) async fn find_file_detail(&self, file_id: &str) -> Result<crate::bmc::FileDetail> {
        let (mm, db_id) = self.get_db().await?;
        let dataset = GenericCrudService::<FileDetailBmc>::get(
            mm,
            &db_id,
            None,
            Value::String(file_id.to_string()),
        )
        .await
        .map_err(Error::from)?;

        Self::dataset_to_file_detail(&dataset)
            .ok_or_else(|| Error::NotFoundError(format!("文件不存在: {}", file_id)))
    }

    /// 创建文件数据库记录。
    ///
    /// 将 `FileInfo` 中的所有字段（包括缩略图信息）写入 `cmx_file_detail` 表。
    ///
    /// # Arguments
    ///
    /// * `file_info` - 文件信息，缩略图字段为 `None` 时对应列写入 NULL。
    /// * `_request` - 原始上传请求（保留用于后续扩展）。
    /// * `md5_hash` - 文件 MD5 哈希值，存入 `hash_info` 字段。
    ///
    /// # Returns
    ///
    /// 成功时返回从数据库回读的 `FileDetail` 记录。
    ///
    /// # Errors
    ///
    /// 当数据库插入失败时返回 `UploadError`。
    pub(super) async fn create_file_record(
        &self,
        file_info: &FileInfo,
        _request: &UploadRequest,
        md5_hash: &str,
    ) -> Result<crate::bmc::FileDetail> {
        let (mm, db_id) = self.get_db().await?;
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
            th_url: file_info.th_url.clone(),
            th_filename: file_info.th_filename.clone(),
            th_size: file_info.th_size,
            th_content_type: file_info.th_content_type.clone(),
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

    /// 尝试秒传，根据 `hash_info` 和 `platform` 查找已有文件。
    ///
    /// 在数据库中查找具有相同 MD5 哈希且相同存储平台的未归档文件，
    /// 若存在则可直接复用其存储路径，无需重复上传物理文件。
    ///
    /// # Arguments
    ///
    /// * `hash_info` - 文件的 JSON 格式哈希信息（包含 MD5）。
    /// * `platform` - 存储平台标识。
    ///
    /// # Returns
    ///
    /// 找到匹配文件时返回 `Some(FileInfo)`，否则返回 `None`。
    pub(super) async fn try_instant_upload(
        &self,
        hash_info: &str,
        platform: &str,
    ) -> Result<Option<FileInfo>> {
        let (mm, db_id) = self.get_db().await?;
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
            mm,
            &db_id,
            None,
            Some(vec![filter]),
            list_options,
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
