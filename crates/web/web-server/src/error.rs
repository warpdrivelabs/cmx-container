//! Web 服务器错误模块
use thiserror::Error;
/// 结果类型，包含成功值或错误
pub type Result<T> = core::result::Result<T, Error>;

/// Web 服务器错误类型
#[derive(Error, Debug)]
pub enum Error {
	#[error("配置错误: {0}")]
	ConfigError(String),

	#[error("服务器设置错误: {0}")]
	ServerSetup(String),
}

