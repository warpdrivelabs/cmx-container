//! 存储服务层模块
//!
//! 提供组合存储后端和数据库操作的高级文件服务。
//! 参考 Java FileStorageService 的设计模式。
//!
//! 本模块按职责拆分为多个子模块，结构体定义与 trait 定义保留在此处，
//! 各功能方法的 `impl` 块分散到子模块中：
//!
//! - [`helpers`]：内部辅助方法（哈希计算、数据集映射、文件记录 CRUD 等）
//! - [`thumbnail`]：缩略图生成
//! - [`upload`]：文件上传
//! - [`download`]：文件下载与缩略图下载
//! - [`delete`]：文件删除与批量删除
//! - [`query`]：文件查询（信息获取、列表分页、存在性检查）
//! - [`presign`]：预签名 URL 生成（上传/下载）
//! - [`copy`]：文件复制
//! - [`multipart`]：分片上传相关操作

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

use crate::error::Result;
use crate::manager::StorageManager;
use crate::types::*;

mod copy;
mod delete;
mod download;
mod helpers;
mod multipart;
mod presign;
mod query;
mod thumbnail;
mod upload;

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
    async fn presign_upload(
        &self,
        request: PresignUploadRequest,
        expires: Duration,
    ) -> Result<PresignUploadResult>;

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
    async fn init_multipart_upload(
        &self,
        request: MultipartInitRequest,
    ) -> Result<MultipartSession>;

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

/// 默认存储服务实现。
///
/// 组合 `StorageManager`（多平台后端管理）和 `GenericCrudService`（数据库操作），
/// 提供 `StorageService` trait 的完整实现。
pub struct DefaultStorageService {
    /// 存储管理器，管理多个存储后端实例
    manager: Arc<StorageManager>,
    /// 数据库管理器
    db: &'static cmx_database::DatabaseManager,
}

impl DefaultStorageService {
    /// 创建默认存储服务。
    ///
    /// # Arguments
    ///
    /// * `manager` - 存储管理器实例，管理所有已配置的存储后端。
    /// * `db` - 数据库管理器实例。
    pub fn new(manager: Arc<StorageManager>, db: &'static cmx_database::DatabaseManager) -> Self {
        Self { manager, db }
    }
}

/// `StorageService` 的唯一实现。
///
/// 各方法体委托给按职责拆分到子模块（[`upload`] / [`download`] / [`delete`] /
/// [`query`] / [`presign`] / [`copy`] / [`multipart`]）中的固有方法。
/// Rust 要求一个类型对同一 trait 只能有一个 `impl`，因此这里集中委派，
/// 实现逻辑分散在各子模块的 `impl DefaultStorageService` 块中。
#[async_trait]
impl StorageService for DefaultStorageService {
    async fn upload(&self, request: UploadRequest) -> Result<FileInfo> {
        self.upload(request).await
    }

    async fn download(&self, file_id: &str) -> Result<FileDownload> {
        self.download(file_id).await
    }

    async fn download_thumbnail(&self, file_id: &str) -> Result<FileDownload> {
        self.download_thumbnail(file_id).await
    }

    async fn delete(&self, file_id: &str) -> Result<()> {
        self.delete(file_id).await
    }

    async fn batch_delete(&self, file_ids: &[String]) -> Result<Vec<Result<()>>> {
        self.batch_delete(file_ids).await
    }

    async fn get_file_info(&self, file_id: &str) -> Result<FileInfo> {
        self.get_file_info(file_id).await
    }

    async fn list_files(&self, query: FileQuery) -> Result<FilePage> {
        self.list_files(query).await
    }

    async fn exists(&self, file_id: &str) -> Result<bool> {
        self.exists(file_id).await
    }

    async fn presign_download(&self, file_id: &str, expires: Duration) -> Result<String> {
        self.presign_download(file_id, expires).await
    }

    async fn presign_upload(
        &self,
        request: PresignUploadRequest,
        expires: Duration,
    ) -> Result<PresignUploadResult> {
        self.presign_upload(request, expires).await
    }

    async fn copy_file(&self, file_id: &str, target_platform: Option<&str>) -> Result<FileInfo> {
        self.copy_file(file_id, target_platform).await
    }

    async fn init_multipart_upload(
        &self,
        request: MultipartInitRequest,
    ) -> Result<MultipartSession> {
        self.init_multipart_upload(request).await
    }

    async fn upload_part(&self, session_id: &str, part: PartData) -> Result<PartInfo> {
        self.upload_part(session_id, part).await
    }

    async fn complete_multipart_upload(&self, session_id: &str) -> Result<FileInfo> {
        self.complete_multipart_upload(session_id).await
    }

    async fn abort_multipart_upload(&self, session_id: &str) -> Result<()> {
        self.abort_multipart_upload(session_id).await
    }
}
