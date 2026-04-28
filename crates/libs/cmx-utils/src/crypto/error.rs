//! 加解密模块错误类型定义
use thiserror::Error;

/// 加解密模块的结果类型别名
pub type Result<T> = core::result::Result<T, Error>;

/// 加解密模块的自定义错误枚举
#[derive(Error, Debug)]
pub enum Error {
    #[error("加密操作失败: {0}")]
    EncryptionFailed(String),
    #[error("解密操作失败: {0}")]
    DecryptionFailed(String),
    #[error("加密格式无效（不匹配任何已注册算法）: {0}")]
    InvalidFormat(String),
    #[error("全局实例未初始化")]
    NotInitialized,
}
