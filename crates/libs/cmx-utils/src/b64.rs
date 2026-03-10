// 导入 base64 库的通用引擎和引擎特性
use base64::engine::{general_purpose, Engine};

/// 使用 URL 安全的 Base64 编码方案对内容进行编码
/// 
/// # 参数
/// 
/// * `content` - 实现了 AsRef<[u8]> 特性的内容，可以是字符串、字节数组等
/// 
/// # 返回值
/// 
/// 返回 URL 安全的 Base64 编码字符串（不包含填充字符）
pub fn b64u_encode(content: impl AsRef<[u8]>) -> String {
	general_purpose::URL_SAFE_NO_PAD.encode(content)
}

/// 使用 URL 安全的 Base64 编码方案对字符串进行解码
/// 
/// # 参数
/// 
/// * `b64u` - URL 安全的 Base64 编码字符串
/// 
/// # 返回值
/// 
/// 成功时返回解码后的字节数组(Vec<u8>)，失败时返回 FailToB64uDecode 错误
pub fn b64u_decode(b64u: &str) -> Result<Vec<u8>> {
	general_purpose::URL_SAFE_NO_PAD
		.decode(b64u)
		.map_err(|_| Error::FailToB64uDecode)
}

/// 使用 URL 安全的 Base64 编码方案对字符串进行解码，并转换为 UTF-8 字符串
/// 
/// # 参数
/// 
/// * `b64u` - URL 安全的 Base64 编码字符串
/// 
/// # 返回值
/// 
/// 成功时返回解码后的 UTF-8 字符串，失败时返回 FailToB64uDecode 错误
pub fn b64u_decode_to_string(b64u: &str) -> Result<String> {
	b64u_decode(b64u)
		.ok()
		.and_then(|r| String::from_utf8(r).ok())
		.ok_or(Error::FailToB64uDecode)
}

// region:    --- 错误定义

/// 定义 Base64 处理模块的结果类型别名
pub type Result<T> = core::result::Result<T, Error>;

/// Base64 处理模块的自定义错误枚举
#[derive(Debug)]
pub enum Error {
	/// Base64 解码失败错误
	FailToB64uDecode,
}

// region:    --- 错误实现样板代码
impl core::fmt::Display for Error {
	fn fmt(
		&self,
		fmt: &mut core::fmt::Formatter,
	) -> core::result::Result<(), core::fmt::Error> {
		write!(fmt, "{self:?}")
	}
}

impl std::error::Error for Error {}
// endregion: --- 错误实现样板代码

// endregion: --- 错误定义
