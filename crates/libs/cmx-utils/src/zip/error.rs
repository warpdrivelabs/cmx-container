//! # ZIP 操作错误定义

use thiserror::Error;

/// ZIP 操作错误类型
#[derive(Error, Debug)]
pub enum ZipError {
    /// IO 错误
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    /// ZIP 压缩/解压错误
    #[error("ZIP 错误: {0}")]
    Zip(#[from] zip::result::ZipError),

    /// 路径错误
    #[error("路径错误: {0}")]
    Path(String),

    /// 文件不存在
    #[error("文件不存在: {0}")]
    FileNotFound(String),

    /// 不是有效的目录
    #[error("不是有效的目录: {0}")]
    NotDirectory(String),

    /// 不是有效的文件
    #[error("不是有效的文件: {0}")]
    NotFile(String),

    /// 压缩源为空
    #[error("压缩源为空")]
    EmptySource,

    /// 创建输出文件失败
    #[error("创建输出文件失败: {0}")]
    CreateOutputFailed(String),
}
