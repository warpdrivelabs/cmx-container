//! Argon2id 密码哈希器。
//!
//! 封装 `argon2` crate 的哈希与校验逻辑，支持自定义内存/时间/并行度参数。

use argon2::password_hash::SaltString;
use argon2::password_hash::rand_core::OsRng;
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};

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
        .map_err(|e| {
            AuthInfraError::Auth(cmx_traits::auth::AuthError::PasswordHashError(format!(
                "Argon2 参数无效: {}",
                e
            )))
        })?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Argon2Config;

    /// 构造一个低成本的 Argon2 配置（减少内存/时间开销以加速测试）。
    ///
    /// 生产环境使用的默认参数（64MB 内存 + time_cost=3）会让单次哈希耗时数百毫秒，
    /// 测试场景下使用最小参数（16KB 内存 + time_cost=1）即可。
    fn make_test_hasher() -> Argon2Hasher {
        let config = Argon2Config {
            memory_cost: 8, // 最小内存（KB）
            time_cost: 1,
            parallelism: 1,
        };
        Argon2Hasher::new(&config).expect("Argon2Hasher 构造失败")
    }

    #[test]
    fn test_argon2_hash_and_verify_success() {
        let hasher = make_test_hasher();
        let plain = "MySecureP@ssw0rd!";

        let hash = hasher.hash(plain).expect("哈希密码失败");

        // PHC 格式应包含 argon2id 算法标识
        assert!(
            hash.starts_with("$argon2"),
            "哈希字符串应以 $argon2 开头，实际: {}",
            hash
        );

        // 校验通过
        let verified = hasher.verify(plain, &hash).expect("校验密码失败");
        assert!(verified, "正确密码应校验通过");
    }

    #[test]
    fn test_argon2_verify_wrong_password_fails() {
        let hasher = make_test_hasher();
        let hash = hasher.hash("correct-password").expect("哈希密码失败");

        // 错误密码应校验失败（返回 Ok(false)）
        let verified = hasher
            .verify("wrong-password", &hash)
            .expect("校验密码不应返回 Err");
        assert!(!verified, "错误密码应校验失败");
    }

    #[test]
    fn test_argon2_same_password_different_hash() {
        let hasher = make_test_hasher();
        let plain = "SamePassword123!";

        // 相同密码哈希两次
        let hash1 = hasher.hash(plain).expect("哈希密码 1 失败");
        let hash2 = hasher.hash(plain).expect("哈希密码 2 失败");

        // 1. 两次哈希字符串不同（盐值不同）
        assert_ne!(
            hash1, hash2,
            "相同密码的两次哈希应不同（盐值随机），实际相同: {}",
            hash1
        );

        // 2. 两个哈希都能正确校验通过
        assert!(
            hasher.verify(plain, &hash1).expect("校验 hash1 失败"),
            "hash1 应校验通过"
        );
        assert!(
            hasher.verify(plain, &hash2).expect("校验 hash2 失败"),
            "hash2 应校验通过"
        );
    }

    #[test]
    fn test_argon2_verify_invalid_hash_format_returns_err() {
        let hasher = make_test_hasher();

        // 非法的哈希字符串格式
        let result = hasher.verify("any-password", "not-a-valid-hash-format");
        assert!(
            result.is_err(),
            "非法哈希格式应返回 Err，实际: {:?}",
            result
        );

        // 空字符串也是非法格式
        let result = hasher.verify("any-password", "");
        assert!(
            result.is_err(),
            "空哈希字符串应返回 Err，实际: {:?}",
            result
        );
    }

    #[test]
    fn test_argon2_hash_empty_password() {
        let hasher = make_test_hasher();

        // 空密码也能被哈希（Argon2 允许空输入）
        let hash = hasher.hash("").expect("哈希空密码失败");
        assert!(hash.starts_with("$argon2"));

        assert!(
            hasher.verify("", &hash).expect("校验空密码失败"),
            "空密码应校验通过"
        );
        assert!(
            !hasher.verify("nonempty", &hash).expect("校验非空密码失败"),
            "非空密码不应匹配空密码的哈希"
        );
    }

    #[test]
    fn test_argon2_invalid_params_returns_err() {
        // 不合理的参数（memory_cost = 0）应导致构造失败
        let config = Argon2Config {
            memory_cost: 0,
            time_cost: 0,
            parallelism: 0,
        };
        let result = Argon2Hasher::new(&config);
        assert!(result.is_err(), "无效 Argon2 参数应导致构造失败");
    }
}
