//! PKCE（Proof Key for Code Exchange）验证。
//!
//! 实现 RFC 7636 PKCE 扩展，防止授权码拦截攻击。

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};

/// PKCE 验证器。
pub struct PkceVerifier;

impl PkceVerifier {
    /// 验证 PKCE `code_verifier` 与 `code_challenge` 是否匹配。
    ///
    /// 算法：`BASE64URL(SHA256(code_verifier)) == code_challenge`。
    ///
    /// # Arguments
    ///
    /// * `code_verifier` - 客户端生成的随机验证字符串。
    /// * `code_challenge` - 授权请求时发送的挑战值。
    /// * `method` - 验证方法：`S256` 或 `plain`。
    ///
    /// # Returns
    ///
    /// 匹配时返回 `true`，不匹配或方法不支持时返回 `false`。
    pub fn verify(code_verifier: &str, code_challenge: &str, method: &str) -> bool {
        match method {
            "S256" => {
                let computed = Self::compute_challenge(code_verifier);
                computed == code_challenge
            }
            "plain" => code_verifier == code_challenge,
            _ => false,
        }
    }

    /// 从 `code_verifier` 计算 `code_challenge`（S256 方法）。
    ///
    /// # Arguments
    ///
    /// * `code_verifier` - 客户端生成的随机验证字符串。
    ///
    /// # Returns
    ///
    /// 返回 `BASE64URL(SHA256(code_verifier))` 字符串。
    pub fn compute_challenge(code_verifier: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(code_verifier.as_bytes());
        let hash = hasher.finalize();
        URL_SAFE_NO_PAD.encode(hash)
    }

    /// 生成随机的 `code_verifier`。
    ///
    /// # Returns
    ///
    /// 返回 32 字节随机数的 Base64URL 编码字符串。
    pub fn generate_code_verifier() -> String {
        let bytes: [u8; 32] = rand::random();
        URL_SAFE_NO_PAD.encode(bytes)
    }

    /// 从 `code_verifier` 生成 `code_challenge`。
    ///
    /// # Arguments
    ///
    /// * `code_verifier` - 客户端生成的随机验证字符串。
    ///
    /// # Returns
    ///
    /// 返回 `BASE64URL(SHA256(code_verifier))` 字符串。
    pub fn generate_challenge(code_verifier: &str) -> String {
        Self::compute_challenge(code_verifier)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pkce_s256_verify() {
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        // RFC 7636 附录 B 测试向量
        let expected_challenge = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";
        let computed = PkceVerifier::compute_challenge(verifier);
        assert_eq!(computed, expected_challenge);
        assert!(PkceVerifier::verify(verifier, expected_challenge, "S256"));
    }

    #[test]
    fn test_pkce_plain_verify() {
        let verifier = "my-code-verifier";
        assert!(PkceVerifier::verify(verifier, verifier, "plain"));
    }

    #[test]
    fn test_pkce_generate_roundtrip() {
        let verifier = PkceVerifier::generate_code_verifier();
        let challenge = PkceVerifier::generate_challenge(&verifier);
        assert!(PkceVerifier::verify(&verifier, &challenge, "S256"));
    }

    #[test]
    fn test_pkce_invalid_method() {
        assert!(!PkceVerifier::verify("verifier", "challenge", "unknown"));
    }
}
