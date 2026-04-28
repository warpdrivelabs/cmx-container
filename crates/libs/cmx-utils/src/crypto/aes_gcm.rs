//! AES-256-GCM 认证加密算法实现

use aes_gcm::KeyInit;

use crate::crypto::cipher::Cipher;
use crate::crypto::cipher::CipherMeta;
use crate::crypto::error::{Error, Result};

/// AES-256-GCM 算法常量
mod consts {
	/// AES-256-GCM 密钥长度（32 字节 = 256 位）
	pub const KEY_LEN: usize = 32;
	/// Nonce 长度（12 字节 = 96 位），AES-GCM 推荐值
	pub const NONCE_LEN: usize = 12;
}

/// AES-256-GCM 认证加密算法实现
///
/// 使用 AES-256 配合 GCM 模式，提供：
/// - **机密性**：通过 AES 加密保证
/// - **完整性**：通过 GCM 认证标签保证
/// - **认证**：密文被篡改会解密失败
///
/// # 密文格式
///
/// `ENC(AESGCM(NONCE.CIPHERTEXT))`
pub struct Aes256GcmCipher {
	cipher: aes_gcm::Aes256Gcm,
}

impl Aes256GcmCipher {
	/// 使用指定密钥创建 AES-256-GCM 加密器
	///
	/// 密钥会经过规范化处理：
	/// - 不足 32 字节：末尾填充 0
	/// - 超过 32 字节：截断到前 32 字节
	///
	/// # 参数
	/// * `key` - 加密密钥字符串
	pub fn new(key: &str) -> Self {
		let normalized_key = Self::normalize_key(key);
		let key = aes_gcm::Key::<aes_gcm::Aes256Gcm>::from_slice(&normalized_key);
		let cipher = aes_gcm::Aes256Gcm::new(key);
		Self { cipher }
	}

	/// 规范化密钥长度为 32 字节
	///
	/// - 不足 32 字节：末尾用 0 填充
	/// - 超过 32 字节：截断到前 32 字节
	fn normalize_key(key: &str) -> [u8; consts::KEY_LEN] {
		let mut normalized = [0u8; consts::KEY_LEN];
		let key_bytes = key.as_bytes();
		let copy_len = key_bytes.len().min(consts::KEY_LEN);
		normalized[..copy_len].copy_from_slice(&key_bytes[..copy_len]);
		normalized
	}
}

impl Cipher for Aes256GcmCipher {
	fn meta(&self) -> CipherMeta {
		CipherMeta {
			name: "AES-256-GCM",
			prefix: "AESGCM(",
		}
	}

	fn encrypt(&self, plaintext: &str) -> Result<String> {
		use aes_gcm::aead::Aead;
		use base64::engine::general_purpose::STANDARD as BASE64;
		use base64::Engine;
		use rand::Rng;

		let nonce_bytes: [u8; consts::NONCE_LEN] = rand::thread_rng().r#gen();
		let nonce = aes_gcm::Nonce::from_slice(&nonce_bytes);

		let ciphertext = self
			.cipher
			.encrypt(nonce, plaintext.as_bytes())
			.map_err(|e| Error::EncryptionFailed(format!("AES-256-GCM 加密失败: {e}")))?;

		let nonce_b64 = BASE64.encode(nonce_bytes);
		let ciphertext_b64 = BASE64.encode(&ciphertext);

		Ok(format!("AESGCM({nonce_b64}.{ciphertext_b64})"))
	}

	fn decrypt(&self, ciphertext: &str) -> Result<String> {
		use aes_gcm::aead::Aead;
		use base64::engine::general_purpose::STANDARD as BASE64;
		use base64::Engine;

		if !self.is_my_format(ciphertext) {
			return Err(Error::InvalidFormat(format!(
				"密文格式不匹配，期望包含 {} 前缀",
				self.meta().prefix
			)));
		}

		let enc_prefix = "ENC(";
		let enc_suffix = ")";

		let inner = ciphertext
			.strip_prefix(enc_prefix)
			.and_then(|s| s.strip_suffix(enc_suffix))
			.ok_or_else(|| Error::InvalidFormat("加密格式无效：缺少 ENC(...) 包装".to_string()))?;

		let encrypted_part = inner
			.strip_prefix("AESGCM(")
			.and_then(|s| s.strip_suffix(')'))
			.ok_or_else(|| Error::InvalidFormat("加密格式无效：缺少 AESGCM(...) 包装".to_string()))?;

		let parts: Vec<&str> = encrypted_part.split('.').collect();
		if parts.len() != 2 {
			return Err(Error::InvalidFormat(
				"加密格式无效：应为 NONCE.CIPHERTEXT".to_string(),
			));
		}

		let nonce_bytes = BASE64
			.decode(parts[0])
			.map_err(|e| Error::DecryptionFailed(format!("Nonce Base64 解码失败: {e}")))?;

		let ciphertext_bytes = BASE64.decode(parts[1]).map_err(|e| {
			Error::DecryptionFailed(format!("密文 Base64 解码失败: {e}"))
		})?;

		let nonce = aes_gcm::Nonce::from_slice(&nonce_bytes);

		let plaintext = self
			.cipher
			.decrypt(nonce, ciphertext_bytes.as_ref())
			.map_err(|e| Error::DecryptionFailed(format!("AES-256-GCM 解密失败: {e}")))?;

		String::from_utf8(plaintext)
			.map_err(|e| Error::DecryptionFailed(format!("解密结果不是有效的 UTF-8: {e}")))
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_cipher_meta() {
		let cipher = Aes256GcmCipher::new("test-key-32-bytes-long!!!!!");
		let meta = cipher.meta();
		assert_eq!(meta.name, "AES-256-GCM");
		assert_eq!(meta.prefix, "AESGCM(");
	}

	#[test]
	fn test_is_my_format() {
		let cipher = Aes256GcmCipher::new("test-key-32-bytes-long!!!!!");
		assert!(cipher.is_my_format("ENC(AESGCM(abc123))"));
		assert!(!cipher.is_my_format("ENC(CHACHA(abc123))"));
		assert!(!cipher.is_my_format("plaintext"));
	}

	#[test]
	fn test_encrypt_returns_inner_format() {
		let cipher = Aes256GcmCipher::new("test-key-32-bytes-long!!!!!");
		let inner = cipher.encrypt("hello").unwrap();
		assert!(inner.starts_with("AESGCM("));
		assert!(inner.ends_with(")"));
		assert!(!inner.starts_with("ENC("));
	}

	#[test]
	fn test_decrypt_invalid_format() {
		let cipher = Aes256GcmCipher::new("test-key-32-bytes-long!!!!!");
		let result = cipher.decrypt("ENC(CHACHA(abc123))");
		assert!(result.is_err());
		assert!(matches!(result.unwrap_err(), Error::InvalidFormat(_)));
	}

	#[test]
	fn test_encrypt_decrypt_roundtrip() {
		let cipher = Aes256GcmCipher::new("test-key-32-bytes-long!!!!!");
		let plaintext = "hello, world!";
		let encrypted = cipher.encrypt(plaintext).unwrap();
		let decrypted = cipher.decrypt(&format!("ENC({encrypted})")).unwrap();
		assert_eq!(decrypted, plaintext);
	}

	#[test]
	fn test_encrypt_decrypt_unicode() {
		let cipher = Aes256GcmCipher::new("test-key-32-bytes-long!!!!!");
		let chinese = "你好，世界！这是一段中文测试文本。";
		let encrypted = cipher.encrypt(chinese).unwrap();
		let decrypted = cipher.decrypt(&format!("ENC({encrypted})")).unwrap();
		assert_eq!(decrypted, chinese);
	}

	#[test]
	fn test_key_normalization_short() {
		let cipher = Aes256GcmCipher::new("short");
		let encrypted = cipher.encrypt("test").unwrap();
		let decrypted = cipher.decrypt(&format!("ENC({encrypted})")).unwrap();
		assert_eq!(decrypted, "test");
	}

	#[test]
	fn test_key_normalization_long() {
		let cipher = Aes256GcmCipher::new("this-is-a-very-long-key-that-exceeds-32-bytes!!");
		let encrypted = cipher.encrypt("test").unwrap();
		let decrypted = cipher.decrypt(&format!("ENC({encrypted})")).unwrap();
		assert_eq!(decrypted, "test");
	}
}
