//! 插件错误类型

use thiserror::Error;

#[derive(Error, Debug)]
pub enum PluginError {
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON 解析错误: {0}")]
    Json(#[from] serde_json::Error),
    #[error("ZIP 错误: {0}")]
    Zip(String),
    #[error("插件错误: {0}")]
    Plugin(String),
    #[error("签名验证失败: {0}")]
    SignatureVerification(String),
}
