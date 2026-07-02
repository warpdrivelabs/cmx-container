//! 存储后端抽象与实现
//!
//! 定义 `StorageBackend` trait 作为底层存储 I/O 的统一抽象接口，
//! 并提供 Local 和 S3 两种具体实现。

pub mod local;
pub mod s3;

use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;

use crate::error::Result;
use crate::types::{
    ListEntry, ListOptions, ObjectMetadata, StorageCapabilities, WriteOptions, WriteResult,
};

/// 存储后端统一抽象接口
///
/// 封装 OpenDAL Operator，提供纯粹的存储 I/O 操作。
/// 不涉及数据库操作，仅负责与存储服务的直接交互。
///
/// # Implementations
///
/// - [`local::LocalBackend`][]: 本地文件系统存储
/// - [`s3::S3Backend`]: Amazon S3 及兼容存储
#[async_trait]
pub trait StorageBackend: Send + Sync + 'static {
    /// 获取后端类型标识
    ///
    /// # Returns
    ///
    /// 返回后端类型字符串，如 `"local"` 或 `"s3"`。
    fn backend_type(&self) -> &str;

    /// 获取平台标识
    ///
    /// # Returns
    ///
    /// 返回配置的平台唯一标识。
    fn platform(&self) -> &str;

    /// 获取存储能力
    ///
    /// # Returns
    ///
    /// 返回该存储后端支持的操作能力列表。
    fn capabilities(&self) -> &StorageCapabilities;

    /// 写入文件
    ///
    /// # Arguments
    ///
    /// * `path` - 文件存储路径
    /// * `data` - 文件二进制数据
    /// * `opts` - 写入选项（Content-Type、缓存策略等）
    ///
    /// # Returns
    ///
    /// 成功时返回写入结果，包含 ETag 和实际写入长度。
    ///
    /// # Errors
    ///
    /// 当写入失败时返回错误。
    async fn write(&self, path: &str, data: Bytes, opts: WriteOptions) -> Result<WriteResult>;

    /// 读取文件全部内容
    ///
    /// # Arguments
    ///
    /// * `path` - 文件存储路径
    ///
    /// # Returns
    ///
    /// 成功时返回文件的完整二进制数据。
    ///
    /// # Errors
    ///
    /// 当文件不存在或读取失败时返回错误。
    async fn read(&self, path: &str) -> Result<Bytes>;

    /// 获取异步读取器
    ///
    /// # Arguments
    ///
    /// * `path` - 文件存储路径
    ///
    /// # Returns
    ///
    /// 成功时返回异步读取器，可用于流式读取大文件。
    ///
    /// # Errors
    ///
    /// 当文件不存在时返回错误。
    async fn reader(&self, path: &str) -> Result<opendal::Reader>;

    /// 删除文件
    ///
    /// # Arguments
    ///
    /// * `path` - 文件存储路径
    ///
    /// # Returns
    ///
    /// 成功时返回 `Ok(())`。
    ///
    /// # Errors
    ///
    /// 当删除失败时返回错误。
    async fn delete(&self, path: &str) -> Result<()>;

    /// 获取文件元数据
    ///
    /// # Arguments
    ///
    /// * `path` - 文件存储路径
    ///
    /// # Returns
    ///
    /// 成功时返回文件的元数据信息。
    ///
    /// # Errors
    ///
    /// 当文件不存在或获取元数据失败时返回错误。
    async fn stat(&self, path: &str) -> Result<ObjectMetadata>;

    /// 列举文件
    ///
    /// # Arguments
    ///
    /// * `prefix` - 列举路径前缀
    /// * `opts` - 列举选项（最大条目数、是否递归）
    ///
    /// # Returns
    ///
    /// 返回匹配前缀的所有文件和目录条目列表。
    ///
    /// # Errors
    ///
    /// 当列举失败时返回错误。
    async fn list(&self, prefix: &str, opts: ListOptions) -> Result<Vec<ListEntry>>;

    /// 复制文件
    ///
    /// # Arguments
    ///
    /// * `from` - 源文件路径
    /// * `to` - 目标文件路径
    ///
    /// # Returns
    ///
    /// 成功时返回 `Ok(())`。
    ///
    /// # Errors
    ///
    /// 当复制失败或后端不支持此操作时返回错误。
    async fn copy(&self, from: &str, to: &str) -> Result<()>;

    /// 生成读取预签名 URL
    ///
    /// # Arguments
    ///
    /// * `path` - 文件存储路径
    /// * `expires` - 签名有效期
    ///
    /// # Returns
    ///
    /// 返回可直接访问的预签名 URL。
    ///
    /// # Errors
    ///
    /// 当后端不支持预签名或生成失败时返回错误。
    async fn presign_read(&self, path: &str, expires: Duration) -> Result<String>;

    /// 生成写入预签名 URL
    ///
    /// # Arguments
    ///
    /// * `path` - 文件存储路径
    /// * `expires` - 签名有效期
    ///
    /// # Returns
    ///
    /// 返回可直接上传的预签名 URL。
    ///
    /// # Errors
    ///
    /// 当后端不支持预签名或生成失败时返回错误。
    async fn presign_write(&self, path: &str, expires: Duration) -> Result<String>;

    /// 创建目录
    ///
    /// # Arguments
    ///
    /// * `path` - 目录路径
    ///
    /// # Returns
    ///
    /// 成功时返回 `Ok(())`。
    ///
    /// # Errors
    ///
    /// 当创建失败时返回错误。
    async fn create_dir(&self, path: &str) -> Result<()>;

    /// 检查文件是否存在
    ///
    /// # Arguments
    ///
    /// * `path` - 文件存储路径
    ///
    /// # Returns
    ///
    /// 若文件存在则返回 `true`，否则返回 `false`。
    async fn exists(&self, path: &str) -> Result<bool>;
}

/// 根据配置创建对应的存储后端实例
///
/// # Arguments
///
/// * `config` - 存储实例配置
///
/// # Returns
///
/// 成功时返回对应类型的存储后端实例。
///
/// # Errors
///
/// 当配置无效或创建失败时返回错误。
pub fn create_backend(
    config: &crate::config::StorageInstanceConfig,
) -> Result<Box<dyn StorageBackend>> {
    match config.storage_type {
        crate::config::StorageType::Local => {
            let backend = local::LocalBackend::new(config)?;
            Ok(Box::new(backend))
        }
        crate::config::StorageType::S3 => {
            let backend = s3::S3Backend::new(config)?;
            Ok(Box::new(backend))
        }
    }
}
