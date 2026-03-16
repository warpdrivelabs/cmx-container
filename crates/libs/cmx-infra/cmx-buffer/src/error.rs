//! cmx-buffer 模块错误类型定义
//!
//! 定义了模块可能遇到的所有错误类型，包括连接错误、缓存操作错误、分布式锁错误等。

use thiserror::Error;

/// cmx-buffer 模块的错误类型
///
/// 使用 thiserror 库实现，便于错误传播和错误信息格式化。
#[derive(Error, Debug)]
pub enum Error {
    /// 连接相关错误
    #[error("Redis 连接错误: {0}")]
    ConnectionError(String),

    /// 连接池相关错误
    #[error("连接池错误: {0}")]
    PoolError(String),

    /// 缓存操作相关错误
    #[error("缓存操作错误: {0}")]
    OperationError(String),

    /// 序列化/反序列化错误
    #[error("序列化错误: {0}")]
    SerializeError(String),

    /// 分布式锁相关错误
    #[error("分布式锁错误: {0}")]
    LockError(String),

    /// 超时错误
    #[error("操作超时: {0}")]
    TimeoutError(String),

    /// 键类型不匹配错误
    #[error("键类型错误: {0}")]
    KeyTypeError(String),

    /// 配置错误
    #[error("配置错误: {0}")]
    ConfigError(String),

    /// 锁冲突错误
    #[error("锁冲突: {0}")]
    LockConflictError(String),

    /// 未知错误
    #[error("未知错误: {0}")]
    UnknownError(String),
}

/// Result 类型别名
pub type Result<T> = std::result::Result<T, Error>;

/// 从 redis crate 的错误转换为模块错误
impl From<redis::RedisError> for Error {
    #[allow(unreachable_code)]
    fn from(err: redis::RedisError) -> Self {
        let msg = err.to_string();
        if msg.contains("connection") || msg.contains("Connection") {
            return Error::ConnectionError(msg);
        }
        if msg.contains("timeout") || msg.contains("Timeout") || msg.contains("timed out") {
            return Error::TimeoutError(msg);
        }
        if msg.contains("BUSY") || msg.contains("script") {
            return Error::OperationError(msg);
        }
        Error::OperationError(msg)
    }
}

/// 从 serde_json 错误转换为模块错误
impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Error::SerializeError(err.to_string())
    }
}

/// 从 tokio::io::错误转换为模块错误
impl From<tokio::io::Error> for Error {
    fn from(err: tokio::io::Error) -> Self {
        Error::ConnectionError(err.to_string())
    }
}
