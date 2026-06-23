//! Argon2id 密码哈希器。
//!
//! 封装 `argon2` crate 的哈希与校验逻辑，支持自定义内存/时间/并行度参数。

use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::SaltString;

use crate::config::Argon2Config;
use crate::error::{AuthInfraError, Result};

/// Argon2id 密码哈希器。
///
/// 封装 `argon2` crate 的哈希与校验逻辑，支持自定义内存/时间/并行度参数。
#[derive(Clone)]
pub struct Argon2Hasher {
    /// Argon2id 算法实例（参数来自 `Argon2Config`）。
    argon2: Argon2<'static>,
}

impl Argon2Hasher {
    /// 创建新的哈希器。
    ///
    /// # Arguments
    ///
    /// * `config` - Argon2 参数配置（内存成本、时间成本、并行度）。
    ///
    /// # Returns
    ///
    /// 成功时返回构造完成的 `Argon2Hasher` 实例。
    ///
    /// # Errors
    ///
    /// 当 Argon2 参数无效时返回 `AuthInfraError`。
    pub fn new(config: &Argon2Config) -> Result<Self> {
        let params = argon2::Params::new(
            config.memory_cost,
            config.time_cost,
            config.parallelism,
            None,
        )
        .map_err(|e| AuthInfraError::Auth(cmx_traits::auth::AuthError::PasswordHashError(
            format!("Argon2 参数无效: {}", e),
        )))?;

        let argon2 = Argon2::from(params);

        Ok(Self { argon2 })
    }

    /// 哈希明文密码。
    ///
    /// 使用随机盐值对明文密码进行 Argon2id 哈希，返回 PHC 字符串格式。
    ///
    /// # Arguments
    ///
    /// * `plain` - 待哈希的明文密码。
    ///
    /// # Returns
    ///
    /// 成功时返回 PHC 格式的哈希字符串。
    ///
    /// # Errors
    ///
    /// 当哈希计算失败时返回 `AuthInfraError`。
    pub fn hash(&self, plain: &str) -> Result<String> {
        let salt = SaltString::generate(&mut OsRng);
        let hash = self
            .argon2
            .hash_password(plain.as_bytes(), &salt)
            .map_err(|e| {
                AuthInfraError::Auth(cmx_traits::auth::AuthError::PasswordHashError(format!(
                    "密码哈希失败: {}",
                    e
                )))
            })?;
        Ok(hash.to_string())
    }

    /// 校验明文密码是否匹配哈希值。
    ///
    /// # Arguments
    ///
    /// * `plain` - 待校验的明文密码。
    /// * `hash` - PHC 格式的哈希字符串。
    ///
    /// # Returns
    ///
    /// 匹配时返回 `Ok(true)`，不匹配或哈希格式无效时返回 `Ok(false)` 或 `Err`。
    ///
    /// # Errors
    ///
    /// 当哈希字符串格式无效时返回 `AuthInfraError`。
    pub fn verify(&self, plain: &str, hash: &str) -> Result<bool> {
        let parsed_hash = PasswordHash::new(hash)
            .map_err(|_| AuthInfraError::Auth(cmx_traits::auth::AuthError::PasswordVerifyFailed))?;
        Ok(self
            .argon2
            .verify_password(plain.as_bytes(), &parsed_hash)
            .is_ok())
    }
}
