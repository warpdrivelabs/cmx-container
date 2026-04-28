//! # 加密模块
//!
//! 提供 AES-256-GCM 对称加密与解密功能。
//!
//! ## 功能特性
//!
//! - **AES-256-GCM 加密**: 使用业界标准的认证加密算法，同时保证机密性和完整性
//! - **全局单例模式**: 通过 `OnceLock` 实现线程安全的全局实例管理
//! - **向后兼容**: 解密时遇到非加密前缀的字符串会原样返回
//! - **灵活的密钥管理**: 支持从环境变量 `CMX_ENCRYPT_KEY` 或手动注入密钥
//!
//! ## 加密结果格式
//!
//! ```text
//! ENC(BASE64_NONCE.BASE64_CIPHERTEXT.BASE64_TAG)
//! ```
//!
//! ## 快速开始
//!
//! ```rust,ignore
//! use cmx_utils::crypto::CryptoService;
//!
//! // 初始化（通常在应用启动时调用一次）
//! CryptoService::init("my-secret-key-32-bytes-long!!");
//!
//! // 加密
//! let encrypted = CryptoService::global().unwrap().encrypt("hello world").unwrap();
//!
//! // 解密
//! let decrypted = CryptoService::global().unwrap().decrypt(&encrypted).unwrap();
//! ```

use aes_gcm::aead::Aead;
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use rand::Rng;
use std::sync::OnceLock;

// region:    --- 常量定义

/// AES-256-GCM 密钥长度（32 字节 = 256 位）
const KEY_LEN: usize = 32;

/// Nonce 长度（12 字节 = 96 位），AES-GCM 推荐值
const NONCE_LEN: usize = 12;

/// 加密结果的前缀标识，用于区分加密文本和明文
const ENC_PREFIX: &str = "ENC(";

/// 加密结果的后缀标识
const ENC_SUFFIX: &str = ")";

// endregion: --- 常量定义

// region:    --- 错误定义

/// 加密模块的结果类型别名
pub type Result<T> = core::result::Result<T, Error>;

/// 加密模块的自定义错误枚举
#[derive(Debug)]
pub enum Error {
	/// 加密操作失败
	EncryptionFailed(String),
	/// 解密操作失败
	DecryptionFailed(String),
	/// 全局实例未初始化
	NotInitialized,
}

// region:    --- 错误实现样板代码
impl core::fmt::Display for Error {
	fn fmt(&self, fmt: &mut core::fmt::Formatter) -> core::result::Result<(), core::fmt::Error> {
		write!(fmt, "{self:?}")
	}
}

impl std::error::Error for Error {}
// endregion: --- 错误实现样板代码

// endregion: --- 错误定义

/// AES-256-GCM 加密服务
///
/// 提供对称加密与解密功能，使用全局单例模式管理加密实例。
/// 密钥在初始化时确定，之后不可更改。
pub struct CryptoService {
	/// AES-256-GCM 加密器实例
	cipher: Aes256Gcm,
}

impl CryptoService {
	/// 初始化全局 CryptoService 实例
	///
	/// 传入的密钥会经过规范化处理：
	/// - 不足 32 字节：末尾填充 0
	/// - 超过 32 字节：截断到前 32 字节
	///
	/// # 参数
	///
	/// * `key` - 加密密钥字符串
	///
	/// # 注意
	///
	/// 此方法只能调用一次，重复调用会被 `OnceLock` 忽略。
	pub fn init(key: &str) {
		let normalized_key = Self::normalize_key(key);
		let key = aes_gcm::Key::<Aes256Gcm>::from_slice(&normalized_key);
		let cipher = Aes256Gcm::new(key);

		let _ = GLOBAL_CRYPTO.set(Self { cipher });
	}

	/// 使用环境变量 `CMX_ENCRYPT_KEY` 初始化全局实例
	///
	/// 如果环境变量未设置或为空，则使用默认密钥。
	/// 默认密钥仅用于开发环境，生产环境务必设置环境变量。
	pub fn init_from_env() {
		let key = std::env::var("CMX_ENCRYPT_KEY").unwrap_or_else(|_| {
			tracing::warn!("CMX_ENCRYPT_KEY 环境变量未设置，使用默认密钥（仅限开发环境）");
			"cmx-default-encrypt-key!!".to_string()
		});
		Self::init(&key);
	}

	/// 获取全局 CryptoService 实例
	///
	/// # 返回值
	///
	/// 返回全局单例的不可变引用。
	///
	/// # 错误
	///
	/// 如果全局实例尚未初始化，返回 `NotInitialized` 错误。
	pub fn global() -> Result<&'static CryptoService> {
		GLOBAL_CRYPTO
			.get()
			.ok_or(Error::NotInitialized)
	}

	/// 加密明文字符串
	///
	/// 使用 AES-256-GCM 算法加密明文，返回格式为 `ENC(BASE64_NONCE.BASE64_CIPHERTEXT.BASE64_TAG)`。
	///
	/// # 参数
	///
	/// * `plaintext` - 待加密的明文字符串
	///
	/// # 返回值
	///
	/// 成功时返回加密后的字符串，失败时返回 `EncryptionFailed` 错误。
	pub fn encrypt(&self, plaintext: &str) -> Result<String> {
		// 生成 96 位随机 Nonce
		let nonce_bytes: [u8; NONCE_LEN] = rand::thread_rng().r#gen();
		let nonce = Nonce::from_slice(&nonce_bytes);

		// 使用 AES-256-GCM 加密，结果中包含密文和认证标签
		let ciphertext = self
			.cipher
			.encrypt(nonce, plaintext.as_bytes())
			.map_err(|e| Error::EncryptionFailed(format!("AES-256-GCM 加密失败: {e}")))?;

		// 分别对 nonce、密文+标签 进行 Base64 编码
		let nonce_b64 = BASE64.encode(nonce_bytes);
		let ciphertext_b64 = BASE64.encode(&ciphertext);

		// 拼接为最终格式：ENC(NONCE.CIPHERTEXT)
		// 注意：AES-GCM 的 encrypt 输出已经包含了 ciphertext || tag
		Ok(format!("{ENC_PREFIX}{nonce_b64}.{ciphertext_b64}{ENC_SUFFIX}"))
	}

	/// 解密密文字符串
	///
	/// 解密 `ENC(BASE64_NONCE.BASE64_CIPHERTEXT.BASE64_TAG)` 格式的密文。
	/// 如果输入字符串不是以 `ENC(` 开头，则原样返回（向后兼容）。
	///
	/// # 参数
	///
	/// * `ciphertext` - 待解密的密文字符串，或明文字符串（向后兼容）
	///
	/// # 返回值
	///
	/// 成功时返回解密后的明文字符串。如果输入不是加密格式，原样返回。
	pub fn decrypt(&self, ciphertext: &str) -> Result<String> {
		// 向后兼容：非加密前缀的字符串直接原样返回
		if !ciphertext.starts_with(ENC_PREFIX) {
			return Ok(ciphertext.to_string());
		}

		// 去除 ENC(...) 包装
		let inner = ciphertext
			.strip_prefix(ENC_PREFIX)
			.and_then(|s| s.strip_suffix(ENC_SUFFIX))
			.ok_or_else(|| {
				Error::DecryptionFailed("加密格式无效：缺少 ENC(...) 包装".to_string())
			})?;

		// 按 '.' 分割，提取 nonce 和密文+标签
		let parts: Vec<&str> = inner.split('.').collect();
		if parts.len() != 2 {
			return Err(Error::DecryptionFailed(
				"加密格式无效：应为 NONCE.CIPHERTEXT".to_string(),
			));
		}

		// Base64 解码 nonce 和密文
		let nonce_bytes = BASE64
			.decode(parts[0])
			.map_err(|e| Error::DecryptionFailed(format!("Nonce Base64 解码失败: {e}")))?;

		let ciphertext_bytes = BASE64.decode(parts[1]).map_err(|e| {
			Error::DecryptionFailed(format!("密文 Base64 解码失败: {e}"))
		})?;

		// 构造 Nonce
		let nonce = Nonce::from_slice(&nonce_bytes);

		// 使用 AES-256-GCM 解密
		let plaintext = self
			.cipher
			.decrypt(nonce, ciphertext_bytes.as_ref())
			.map_err(|e| Error::DecryptionFailed(format!("AES-256-GCM 解密失败: {e}")))?;

		// 将解密后的字节转换为 UTF-8 字符串
		String::from_utf8(plaintext).map_err(|e| {
			Error::DecryptionFailed(format!("解密结果不是有效的 UTF-8: {e}"))
		})
	}

	/// 规范化密钥长度为 32 字节
	///
	/// - 不足 32 字节：末尾用 0 填充
	/// - 超过 32 字节：截断到前 32 字节
	///
	/// # 参数
	///
	/// * `key` - 原始密钥字符串
	///
	/// # 返回值
	///
	/// 长度恰好为 32 字节的密钥字节数组
	fn normalize_key(key: &str) -> [u8; KEY_LEN] {
		let mut normalized = [0u8; KEY_LEN];
		let key_bytes = key.as_bytes();
		let copy_len = key_bytes.len().min(KEY_LEN);
		normalized[..copy_len].copy_from_slice(&key_bytes[..copy_len]);
		normalized
	}
}

// region:    --- 全局单例

/// 全局 CryptoService 实例，使用 OnceLock 保证线程安全的单次初始化
static GLOBAL_CRYPTO: OnceLock<CryptoService> = OnceLock::new();

// endregion: --- 全局单例

// region:    --- 单元测试

#[cfg(test)]
mod tests {
	use super::*;

	/// 测试密钥规范化逻辑（间接验证：短密钥填充后加密解密正常）
	#[test]
	fn test_normalize_key_short() {
		let key = "short";
		// 短密钥初始化后加密解密正常，间接验证密钥规范化正确
		let normalized_key = normalize_key_for_test(key);
		let k = aes_gcm::Key::<Aes256Gcm>::from_slice(&normalized_key);
		let service = CryptoService {
			cipher: Aes256Gcm::new(k),
		};
		let encrypted = service.encrypt("test").unwrap();
		let decrypted = service.decrypt(&encrypted).unwrap();
		assert_eq!(decrypted, "test");
	}

	/// 测试加密和解密的往返一致性
	#[test]
	fn test_encrypt_decrypt_roundtrip() {
		let service = {
			let key = "test-key-for-crypto-module!!"; // 28 字节，会被填充
			let normalized_key = normalize_key_for_test(key);
			let k = aes_gcm::Key::<Aes256Gcm>::from_slice(&normalized_key);
			CryptoService {
				cipher: Aes256Gcm::new(k),
			}
		};

		let plaintext = "hello, world!";
		let encrypted = service.encrypt(plaintext).unwrap();

		// 加密结果应该有 ENC(...) 包装
		assert!(encrypted.starts_with("ENC("));
		assert!(encrypted.ends_with(")"));

		// 解密后应该得到原文
		let decrypted = service.decrypt(&encrypted).unwrap();
		assert_eq!(decrypted, plaintext);
	}

	/// 测试解密非加密格式字符串时的向后兼容行为
	#[test]
	fn test_decrypt_passthrough() {
		let service = create_test_service();
		let plain = "this is not encrypted";

		// 非加密前缀的字符串应该原样返回
		let result = service.decrypt(plain).unwrap();
		assert_eq!(result, plain);
	}

	/// 测试空字符串的加密解密
	#[test]
	fn test_encrypt_decrypt_empty() {
		let service = create_test_service();
		let encrypted = service.encrypt("").unwrap();
		let decrypted = service.decrypt(&encrypted).unwrap();
		assert_eq!(decrypted, "");
	}

	/// 测试长文本的加密解密
	#[test]
	fn test_encrypt_decrypt_long_text() {
		let service = create_test_service();
		let long_text = "a".repeat(10000);
		let encrypted = service.encrypt(&long_text).unwrap();
		let decrypted = service.decrypt(&encrypted).unwrap();
		assert_eq!(decrypted, long_text);
	}

	/// 测试中文内容的加密解密
	#[test]
	fn test_encrypt_decrypt_unicode() {
		let service = create_test_service();
		let chinese = "你好，世界！这是一段中文测试文本。";
		let encrypted = service.encrypt(chinese).unwrap();
		let decrypted = service.decrypt(&encrypted).unwrap();
		assert_eq!(decrypted, chinese);
	}

	/// 测试每次加密产生不同的密文（因为 Nonce 随机）
	#[test]
	fn test_encrypt_produces_different_ciphertext() {
		let service = create_test_service();
		let plaintext = "same input";

		let encrypted1 = service.encrypt(plaintext).unwrap();
		let encrypted2 = service.encrypt(plaintext).unwrap();

		// 由于 Nonce 是随机的，相同明文的两次加密结果应该不同
		assert_ne!(encrypted1, encrypted2);

		// 但都能正确解密
		assert_eq!(service.decrypt(&encrypted1).unwrap(), plaintext);
		assert_eq!(service.decrypt(&encrypted2).unwrap(), plaintext);
	}

	// region:    --- 测试辅助函数

	/// 创建用于测试的 CryptoService 实例
	fn create_test_service() -> CryptoService {
		let key = "test-key-for-unit-tests!!";
		let normalized_key = normalize_key_for_test(key);
		let k = aes_gcm::Key::<Aes256Gcm>::from_slice(&normalized_key);
		CryptoService {
			cipher: Aes256Gcm::new(k),
		}
	}

	/// 测试用的密钥规范化函数
	fn normalize_key_for_test(key: &str) -> [u8; KEY_LEN] {
		let mut normalized = [0u8; KEY_LEN];
		let key_bytes = key.as_bytes();
		let copy_len = key_bytes.len().min(KEY_LEN);
		normalized[..copy_len].copy_from_slice(&key_bytes[..copy_len]);
		normalized
	}

	// endregion: --- 测试辅助函数
}

// endregion: --- 单元测试
