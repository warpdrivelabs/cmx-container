//! # 加解密模块
//!
//! 提供可扩展的对称加密与解密功能，支持多种加密算法。
//!
//! ## 核心抽象
//!
//! - **Cipher trait**: 定义加密算法的统一接口
//! - **CryptoService**: 加密服务入口，支持运行时切换加密算法
//!
//! ## 内置算法
//!
//! | 算法 | 文件 | 密文格式 |
//! |------|------|---------|
//! | AES-256-GCM | [aes_gcm](aes_gcm) | `ENC(AESGCM(NONCE.CIPHERTEXT))` |
//!
//! ## 扩展新算法
//!
//! 1. 在 `src/crypto/` 下创建新文件，如 `chacha20.rs`
//! 2. 实现 `Cipher` trait
//! 3. 在 `mod.rs` 中添加 `pub mod chacha20;`
//!
//! ```rust,ignore
//! use cmx_utils::crypto::{Cipher, CipherMeta};
//!
//! pub struct ChaCha20PolyCipher { /* ... */ }
//!
//! impl Cipher for ChaCha20PolyCipher {
//!     fn meta(&self) -> CipherMeta {
//!         CipherMeta { name: "ChaCha20-Poly1305", prefix: "CHACHA(" }
//!     }
//!     fn encrypt(&self, p: &str) -> Result<String> { /* ... */ }
//!     fn decrypt(&self, c: &str) -> Result<String> { /* ... */ }
//! }
//! ```
//!
//! ## 快速开始
//!
//! ```rust,ignore
//! use cmx_utils::crypto::{CryptoService, Aes256GcmCipher};
//!
//! // 方式一：使用默认算法（AES-256-GCM）初始化
//! CryptoService::init("my-secret-key-32-bytes-long!!");
//!
//! // 方式二：手动指定算法
//! CryptoService::init_with(Aes256GcmCipher::new("my-key"));
//!
//! // 加密
//! let encrypted = CryptoService::global()?.encrypt("hello world")?;
//! // 输出: ENC(AESGCM(NONCE.CIPHERTEXT))
//!
//! // 解密
//! let decrypted = CryptoService::global()?.decrypt(&encrypted)?;
//! ```

pub mod aes_gcm;
pub mod cipher;
pub mod error;
pub mod service;

pub use aes_gcm::Aes256GcmCipher as Aes256Gcm;
pub use cipher::{Cipher, CipherMeta};
pub use error::{Error, Result};
pub use service::CryptoService;

//endregion: --- 公开导出
