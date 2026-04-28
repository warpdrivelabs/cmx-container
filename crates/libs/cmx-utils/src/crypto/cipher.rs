//! 加解密模块的核心抽象：Cipher trait

use crate::crypto::error::Result;

/// 加密算法的元信息
#[derive(Debug, Clone)]
pub struct CipherMeta {
	/// 算法名称，如 "AES-256-GCM"
	pub name: &'static str,
	/// 密文内层前缀标识，如 "AESGCM("，用于识别具体算法
	pub prefix: &'static str,
}

/// 加密算法接口
///
/// 所有加密算法都应实现此 trait，以便 CryptoService 统一管理。
///
/// # 设计原则
///
/// - 每个算法负责自己的加密/解密逻辑和格式
/// - `encrypt` 返回算法自己的内层格式（如 `AESGCM(NONCE.CIPHERTEXT)`）
/// - `decrypt` 直接接收完整密文（如 `ENC(AESGCM(NONCE.CIPHERTEXT))`），
///   内部自行解析并解密
/// - 算法通过内层前缀（如 `AESGCM(`）识别自己负责的密文格式
pub trait Cipher: Send + Sync {
	/// 返回算法的元信息
	fn meta(&self) -> CipherMeta;

	/// 加密明文
	///
	/// 返回算法特定的内层格式（不包含外层 ENC(...) 包装）。
	///
	/// # 参数
	/// * `plaintext` - 待加密的明文字符串
	///
	/// # 返回值
	/// 成功返回内层格式字符串，如 `AESGCM(NONCE.CIPHERTEXT)`
	fn encrypt(&self, plaintext: &str) -> Result<String>;

	/// 解密密文
	///
	/// 直接接收完整密文（如 `ENC(AESGCM(NONCE.CIPHERTEXT))`），
	/// 检查是否为自己格式，如果是则解密，否则返回错误。
	///
	/// # 参数
	/// * `ciphertext` - 完整的加密字符串（包含 ENC(...) 包装）
	///
	/// # 返回值
	/// 成功返回解密后的明文字符串。如果密文不是当前算法的格式，返回 `InvalidFormat` 错误。
	fn decrypt(&self, ciphertext: &str) -> Result<String>;

	/// 检查密文是否为当前算法的格式
	///
	/// 通过检查密文是否包含算法特定前缀（如 `AESGCM(`）来判断。
	///
	/// # 参数
	/// * `ciphertext` - 完整密文字符串
	///
	/// # 返回值
	/// 如果密文是当前算法的格式返回 true
	fn is_my_format(&self, ciphertext: &str) -> bool {
		ciphertext.contains(self.meta().prefix)
	}
}
