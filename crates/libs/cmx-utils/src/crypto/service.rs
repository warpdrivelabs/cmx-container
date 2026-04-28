//! 加密服务入口

use std::sync::OnceLock;

use crate::crypto::aes_gcm::Aes256GcmCipher;
use crate::crypto::cipher::Cipher;
use crate::crypto::cipher::CipherMeta;
use crate::crypto::error::{Error, Result};

/// 加密结果的最外层前缀标识，所有算法的密文都以此前缀包装
const ENC_PREFIX: &str = "ENC(";

/// 加密结果的后缀标识
const ENC_SUFFIX: &str = ")";

/// 加密服务
///
/// 提供统一的加解密接口，支持运行时切换加密算法。
/// 使用全局单例模式，通过 `OnceLock` 保证线程安全的单次初始化。
///
/// # 密文格式
///
/// 所有算法的密文都统一包装为 `ENC(...)` 格式，
/// 内层包含算法特定的前缀和信息：
///
/// ```text
/// ENC(AESGCM(NONCE.CIPHERTEXT))   <- AES-256-GCM
/// ENC(CHACHA(NONCE.CIPHERTEXT))    <- ChaCha20-Poly1305（未来扩展）
/// ```
///
/// # 初始化方式
///
/// ```rust,ignore
/// // 方式一：使用默认算法（AES-256-GCM）初始化
/// CryptoService::init("my-32-byte-secret-key!!");
///
/// // 方式二：从环境变量读取密钥初始化（默认算法）
/// CryptoService::init_from_env();
///
/// // 方式三：手动指定算法
/// CryptoService::init_with(Aes256GcmCipher::new("my-key"));
/// ```
pub struct CryptoService {
	cipher: Box<dyn Cipher>,
}

impl CryptoService {
	/// 使用默认算法（AES-256-GCM）初始化全局实例
	///
	/// 传入的密钥会经过 AES-256-GCM 的规范化处理。
	///
	/// # 参数
	/// * `key` - 加密密钥字符串
	///
	/// # 注意
	///
	/// 此方法只能调用一次，重复调用会被 `OnceLock` 忽略。
	pub fn init(key: &str) {
		let cipher = Box::new(Aes256GcmCipher::new(key)) as Box<dyn Cipher>;
		let _ = GLOBAL_CRYPTO.set(Self { cipher });
	}

	/// 使用指定算法初始化全局实例
	///
	/// # 示例
	///
	/// ```rust,ignore
	/// CryptoService::init_with(Aes256GcmCipher::new("my-key"));
	/// ```
	pub fn init_with<C: Cipher + 'static>(cipher: C) {
		let _ = GLOBAL_CRYPTO.set(Self {
			cipher: Box::new(cipher) as Box<dyn Cipher>,
		});
	}

	/// 使用环境变量 `CMX_ENCRYPT_KEY` 初始化全局实例（使用默认算法 AES-256-GCM）
	///
	/// 如果环境变量未设置或为空，则使用默认密钥。
	/// 默认密钥仅用于开发环境，生产环境务必设置环境变量。
	pub fn init_from_env() {
		let key = std::env::var("CMX_ENCRYPT_KEY").unwrap_or_else(|_| {
			tracing::warn!(
				"CMX_ENCRYPT_KEY 环境变量未设置，使用默认密钥（仅限开发环境）"
			);
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
		GLOBAL_CRYPTO.get().ok_or(Error::NotInitialized)
	}

	/// 获取当前加密算法的元信息
	///
	/// # 返回值
	///
	/// 返回当前算法的名称和前缀信息。
	pub fn algorithm(&self) -> CipherMeta {
		self.cipher.meta()
	}

	/// 加密明文字符串
	///
	/// 使用当前配置的加密算法加密明文。
	/// 返回格式为 `ENC(ALGO(NONCE.CIPHERTEXT))`。
	///
	/// # 参数
	/// * `plaintext` - 待加密的明文字符串
	///
	/// # 返回值
	///
	/// 成功时返回加密后的字符串，失败时返回错误。
	pub fn encrypt(&self, plaintext: &str) -> Result<String> {
		let inner = self.cipher.encrypt(plaintext)?;
		Ok(format!("{ENC_PREFIX}{inner}{ENC_SUFFIX}"))
	}

	/// 解密密文字符串
	///
	/// 智能识别密文格式并委托给对应算法解密：
	/// - `ENC(...)` 格式：委托给当前 cipher 解密
	/// - 非 `ENC(...)` 格式：原样返回（向后兼容明文数据）
	///
	/// # 参数
	/// * `ciphertext` - 待解密的字符串
	///
	/// # 返回值
	///
	/// 成功时返回解密后的明文字符串。如果输入不是加密格式，原样返回。
	pub fn decrypt(&self, ciphertext: &str) -> Result<String> {
		if !ciphertext.starts_with(ENC_PREFIX) {
			return Ok(ciphertext.to_string());
		}
		self.cipher.decrypt(ciphertext)
	}
}

/// 全局 CryptoService 实例
static GLOBAL_CRYPTO: OnceLock<CryptoService> = OnceLock::new();

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_crypto_service_init() {
		CryptoService::init("test-key-32-bytes-long!!!!!");
		let service = CryptoService::global().unwrap();
		assert_eq!(service.algorithm().name, "AES-256-GCM");
	}

	#[test]
	fn test_crypto_service_encrypt_decrypt() {
		CryptoService::init("test-key-32-bytes-long!!!!!");
		let service = CryptoService::global().unwrap();

		let encrypted = service.encrypt("hello").unwrap();
		assert!(encrypted.starts_with("ENC(AESGCM("));
		assert!(encrypted.ends_with("))"));

		let decrypted = service.decrypt(&encrypted).unwrap();
		assert_eq!(decrypted, "hello");
	}

	#[test]
	fn test_crypto_service_passthrough() {
		CryptoService::init("test-key-32-bytes-long!!!!!");
		let service = CryptoService::global().unwrap();

		let plain = "this is not encrypted";
		let result = service.decrypt(plain).unwrap();
		assert_eq!(result, plain);
	}

	#[test]
	fn test_crypto_service_init_with() {
		CryptoService::init_with(Aes256GcmCipher::new("custom-key-32-bytes-long!!!!!"));
		let service = CryptoService::global().unwrap();
		assert_eq!(service.algorithm().name, "AES-256-GCM");

		let encrypted = service.encrypt("secret").unwrap();
		let decrypted = service.decrypt(&encrypted).unwrap();
		assert_eq!(decrypted, "secret");
	}
}
