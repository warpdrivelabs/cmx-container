//! 存储模块错误类型定义
//!
//! 定义了存储操作可能遇到的所有错误类型，采用 thiserror 库实现，
//! 便于错误溯源和统一处理。

use thiserror::Error;

/// 存储模块的错误类型
///
/// 所有存储相关操作的错误都通过此枚举表示，包括配置错误、I/O 错误、
/// 数据库错误等。使用 `#[error(...)]` 属性定义用户友好的错误消息。
#[derive(Error, Debug)]
pub enum Error {
    /// 配置解析或验证错误
    #[error("配置错误: {0}")]
    ConfigError(String),

    /// 文件上传操作失败
    #[error("上传失败: {0}")]
    UploadError(String),

    /// 文件下载操作失败
    #[error("下载失败: {0}")]
    DownloadError(String),

    /// 文件删除操作失败
    #[error("删除失败: {0}")]
    DeleteError(String),

    /// 请求的文件或资源不存在
    #[error("文件不存在: {0}")]
    NotFoundError(String),

    /// 文件复制操作失败
    #[error("复制失败: {0}")]
    CopyError(String),

    /// 预签名 URL 生成失败
    #[error("预签名失败: {0}")]
    PresignError(String),

    /// 分片上传相关操作失败
    #[error("分片上传错误: {0}")]
    MultipartError(String),

    /// 请求的操作不被当前存储后端支持
    #[error("不支持的操作: {0}")]
    UnsupportedError(String),

    /// 其他存储操作失败
    #[error("存储错误: {0}")]
    StorageError(String),
}

/// 存储操作的结果类型别名
pub type Result<T> = std::result::Result<T, Error>;

impl From<opendal::Error> for Error {
    fn from(err: opendal::Error) -> Self {
        let msg = err.to_string();
        let kind = err.kind();
        match kind {
            opendal::ErrorKind::NotFound => Error::NotFoundError(msg),
            opendal::ErrorKind::Unsupported => Error::UnsupportedError(msg),
            opendal::ErrorKind::ConfigInvalid => Error::ConfigError(msg),
            _ => Error::StorageError(msg),
        }
    }
}

impl From<cmx_database::crud::ServiceError> for Error {
    fn from(err: cmx_database::crud::ServiceError) -> Self {
        Error::StorageError(err.to_string())
    }
}
