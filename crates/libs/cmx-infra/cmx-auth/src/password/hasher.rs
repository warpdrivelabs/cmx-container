//! Argon2id 密码哈希器

use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::SaltString;

use crate::config::Argon2Config;
use crate::error::{AuthInfraError, Result};

/// Argon2id 密码哈希器
#[derive(Clone)]
pub struct Argon2Hasher {
    argon2: Argon2<'static>,
}

impl Argon2Hasher {
    /// 创建新的哈希器
    pub fn new(config: &Argon2Config) -> Result<Self> {
        let params = argon2::Params::new(
            config.memory_cost,
            config.time_cost,
            config.parallelism,
            None,
        )
        .map_err(|e| AuthInfraError::Auth(cmx_traits::AuthError::PasswordHashError(
            format!("Argon2 参数无效: {}", e),
        )))?;

        let argon2 = Argon2::from(params);

        Ok(Self { argon2 })
    }

    /// 哈希密码
    pub fn hash(&self, plain: &str) -> Result<String> {
        let salt = SaltString::generate(&mut OsRng);
        let hash = self
            .argon2
            .hash_password(plain.as_bytes(), &salt)
            .map_err(|e| {
                AuthInfraError::Auth(cmx_traits::AuthError::PasswordHashError(format!(
                    "密码哈希失败: {}",
                    e
                )))
            })?;
        Ok(hash.to_string())
    }

    /// 校验密码
    pub fn verify(&self, plain: &str, hash: &str) -> Result<bool> {
        let parsed_hash = PasswordHash::new(hash)
            .map_err(|_| AuthInfraError::Auth(cmx_traits::AuthError::PasswordVerifyFailed))?;
        Ok(self
            .argon2
            .verify_password(plain.as_bytes(), &parsed_hash)
            .is_ok())
    }
}
