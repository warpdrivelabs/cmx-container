//! 加解密模块错误类型定义

/// 加解密模块的结果类型别名
pub type Result<T> = core::result::Result<T, Error>;

/// 加解密模块的自定义错误枚举
#[derive(Debug)]
pub enum Error {
	/// 加密操作失败
	EncryptionFailed(String),
	/// 解密操作失败
	DecryptionFailed(String),
	/// 加密格式无效（不匹配任何已注册算法）
	InvalidFormat(String),
	/// 全局实例未初始化
	NotInitialized,
}

impl core::fmt::Display for Error {
	fn fmt(&self, fmt: &mut core::fmt::Formatter) -> core::result::Result<(), core::fmt::Error> {
		write!(fmt, "{self:?}")
	}
}

impl std::error::Error for Error {}
