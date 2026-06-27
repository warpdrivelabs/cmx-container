//! JWT 编解码器。

use chrono::Utc;
use jsonwebtoken::{decode, decode_header, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use uuid::Uuid;

use crate::config::AuthConfig;
use crate::error::{AuthInfraError, Result};
use super::claims::{AccessClaims, RefreshClaims};

/// JWT 管理器。
///
/// 负责 JWT Token 的编码与解码，支持 `RS256` / `HS256` 算法与密钥轮换宽限期。
/// 解码时优先使用当前密钥，失败后回退到 `legacy_public_keys` 列表。
pub struct JwtManager {
    /// 认证配置。
    config: AuthConfig,

    /// 当前签名密钥。
    encoding_key: EncodingKey,

    /// 当前验签密钥。
    decoding_key: DecodingKey,

    /// 签名算法（`RS256` / `HS256`）。
    algorithm: Algorithm,

    /// 旧密钥列表（`kid` → `DecodingKey`），用于密钥轮换宽限期内的 Token 验签。
    legacy_decoding_keys: Vec<(String, DecodingKey)>,
}

impl JwtManager {
    /// 创建新的 `JwtManager` 实例。
    ///
    /// 根据配置加载当前签名/验签密钥，以及历史公钥列表（用于密钥轮换宽限期）。
    ///
    /// # Arguments
    ///
    /// * `config` - 认证配置（包含 JWT 算法、密钥、`kid` 等信息）。
    ///
    /// # Returns
    ///
    /// 成功时返回构造完成的 `JwtManager` 实例。
    ///
    /// # Errors
    ///
    /// 当算法不支持、密钥加载失败或密钥文件不可读时返回 `AuthInfraError`。
    pub fn new(config: AuthConfig) -> Result<Self> {
        let algorithm = match config.jwt.algorithm.as_str() {
            "RS256" => Algorithm::RS256,
            "HS256" => Algorithm::HS256,
            other => {
                return Err(AuthInfraError::Auth(cmx_traits::auth::AuthError::InvalidToken(
                    format!("不支持的 JWT 算法: {}", other),
                )))
            }
        };

        let (encoding_key, decoding_key) = Self::load_keys(&config, algorithm)?;
        let legacy_decoding_keys = Self::load_legacy_keys(&config, algorithm)?;

        Ok(Self {
            config,
            encoding_key,
            decoding_key,
            algorithm,
            legacy_decoding_keys,
        })
    }

    /// 编码 Access Token。
    ///
    /// # Arguments
    ///
    /// * `user_id` - 用户 ID。
    /// * `username` - 用户名。
    /// * `roles` - 角色列表。
    /// * `permissions` - 权限列表。
    /// * `org_id` - 组织 ID（可选）。
    /// * `session_id` - 会话 ID。
    /// * `device` - 设备类型。
    ///
    /// # Returns
    ///
    /// 成功时返回签名的 JWT 字符串。
    ///
    /// # Errors
    ///
    /// 当 JWT 编码失败时返回 `AuthInfraError::Jwt`。
    #[allow(clippy::too_many_arguments)]
    pub fn encode_access_token(
        &self,
        user_id: &str,
        username: &str,
        roles: &[String],
        permissions: &[String],
        org_id: Option<&str>,
        session_id: &str,
        device: &str,
    ) -> Result<String> {
        let now = Utc::now().timestamp();
        let jti = Uuid::new_v4().to_string();

        let claims = AccessClaims {
            sub: user_id.to_string(),
            exp: now + self.config.token.access_ttl_secs as i64,
            iat: now,
            jti,
            iss: self.config.jwt.issuer.clone(),
            aud: self.config.jwt.audience.clone(),
            username: username.to_string(),
            roles: roles.to_vec(),
            permissions: permissions.to_vec(),
            org_id: org_id.map(|s| s.to_string()),
            sid: session_id.to_string(),
            device: device.to_string(),
            typ: "access".to_string(),
            kid: self.config.jwt.current_kid.clone(),
        };

        let mut header = Header::new(self.algorithm);
        if let Some(ref kid) = self.config.jwt.current_kid {
            header.kid = Some(kid.clone());
        }

        encode(&header, &claims, &self.encoding_key)
            .map_err(AuthInfraError::Jwt)
    }

    /// 编码 Refresh Token。
    ///
    /// # Arguments
    ///
    /// * `user_id` - 用户 ID。
    /// * `session_id` - 会话 ID。
    /// * `device` - 设备类型。
    ///
    /// # Returns
    ///
    /// 成功时返回签名的 JWT 字符串。
    ///
    /// # Errors
    ///
    /// 当 JWT 编码失败时返回 `AuthInfraError::Jwt`。
    pub fn encode_refresh_token(
        &self,
        user_id: &str,
        session_id: &str,
        device: &str,
    ) -> Result<String> {
        let now = Utc::now().timestamp();
        let jti = Uuid::new_v4().to_string();

        let claims = RefreshClaims {
            sub: user_id.to_string(),
            exp: now + self.config.token.refresh_ttl_secs as i64,
            iat: now,
            jti,
            iss: self.config.jwt.issuer.clone(),
            typ: "refresh".to_string(),
            sid: session_id.to_string(),
            device: device.to_string(),
        };

        let header = Header::new(self.algorithm);
        encode(&header, &claims, &self.encoding_key)
            .map_err(AuthInfraError::Jwt)
    }

    /// 解码 Access Token。
    ///
    /// 优先使用当前密钥验签，失败后回退到 `legacy_public_keys` 列表
    /// （密钥轮换宽限期支持）。
    ///
    /// # Arguments
    ///
    /// * `token` - 待解码的 Access Token 字符串。
    ///
    /// # Returns
    ///
    /// 成功时返回 `AccessClaims`。
    ///
    /// # Errors
    ///
    /// 当 Token 无效、过期或验签失败时返回 `AuthInfraError::Jwt`。
    pub fn decode_access_token(&self, token: &str) -> Result<AccessClaims> {
        self.decode_with_key_fallback(token)
    }

    /// 解码 Refresh Token。
    ///
    /// 优先使用当前密钥验签，失败后回退到 `legacy_public_keys` 列表
    /// （密钥轮换宽限期支持）。
    ///
    /// # Arguments
    ///
    /// * `token` - 待解码的 Refresh Token 字符串。
    ///
    /// # Returns
    ///
    /// 成功时返回 `RefreshClaims`。
    ///
    /// # Errors
    ///
    /// 当 Token 无效、过期或验签失败时返回 `AuthInfraError::Jwt`。
    pub fn decode_refresh_token(&self, token: &str) -> Result<RefreshClaims> {
        self.decode_with_key_fallback(token)
    }

    /// 带密钥轮换宽限期的解码：先尝试当前密钥，再回退到 legacy 密钥
    fn decode_with_key_fallback<T: serde::de::DeserializeOwned>(&self, token: &str) -> Result<T> {
        // 1. 先尝试用当前密钥解码
        if let Ok(token_data) = decode::<T>(token, &self.decoding_key, &self.validation()) {
            return Ok(token_data.claims);
        }

        // 2. 提取 header 中的 kid
        let header = decode_header(token).map_err(AuthInfraError::Jwt)?;
        if let Some(kid) = &header.kid {
            // 3. 在 legacy 列表中查找匹配的密钥
            for (legacy_kid, legacy_key) in &self.legacy_decoding_keys {
                if legacy_kid == kid
                    && let Ok(token_data) = decode::<T>(token, legacy_key, &self.validation()) {
                        return Ok(token_data.claims);
                    }
            }
        }

        Err(AuthInfraError::Jwt(jsonwebtoken::errors::Error::from(
            jsonwebtoken::errors::ErrorKind::InvalidToken,
        )))
    }

    /// 加载编码/解码密钥
    fn load_keys(config: &AuthConfig, algorithm: Algorithm) -> Result<(EncodingKey, DecodingKey)> {
        match algorithm {
            Algorithm::HS256 => {
                let secret = config
                    .jwt
                    .secret
                    .as_deref()
                    .unwrap_or("a7k9m2p4x8q1w5e3r6t0y7u2i9o4p1");
                Ok((
                    EncodingKey::from_secret(secret.as_bytes()),
                    DecodingKey::from_secret(secret.as_bytes()),
                ))
            }
            Algorithm::RS256 => {
                let private_key_pem = config
                    .jwt
                    .private_key
                    .as_deref()
                    .ok_or_else(|| {
                        AuthInfraError::Auth(cmx_traits::auth::AuthError::InvalidToken(
                            "RS256 需要 private_key 配置".to_string(),
                        ))
                    })?;

                let public_key_pem = config
                    .jwt
                    .public_key
                    .as_deref()
                    .ok_or_else(|| {
                        AuthInfraError::Auth(cmx_traits::auth::AuthError::InvalidToken(
                            "RS256 需要 public_key 配置".to_string(),
                        ))
                    })?;

                // 支持文件路径或 PEM 内容
                let private_pem = Self::resolve_key_content(private_key_pem)?;
                let public_pem = Self::resolve_key_content(public_key_pem)?;

                Ok((
                    EncodingKey::from_rsa_pem(private_pem.as_bytes())
                        .map_err(AuthInfraError::Jwt)?,
                    DecodingKey::from_rsa_pem(public_pem.as_bytes())
                        .map_err(AuthInfraError::Jwt)?,
                ))
            }
            _ => Err(AuthInfraError::Auth(cmx_traits::auth::AuthError::InvalidToken(
                format!("不支持的 JWT 算法: {:?}", algorithm),
            ))),
        }
    }

    /// 加载历史公钥列表（密钥轮换宽限期验签）
    fn load_legacy_keys(
        config: &AuthConfig,
        algorithm: Algorithm,
    ) -> Result<Vec<(String, DecodingKey)>> {
        let mut keys = Vec::new();
        for (kid, key_pem) in &config.jwt.legacy_public_keys {
            let pem = Self::resolve_key_content(key_pem)?;
            match algorithm {
                Algorithm::RS256 => {
                    let dk = DecodingKey::from_rsa_pem(pem.as_bytes())
                        .map_err(AuthInfraError::Jwt)?;
                    keys.push((kid.clone(), dk));
                }
                Algorithm::HS256 => {
                    let dk = DecodingKey::from_secret(pem.as_bytes());
                    keys.push((kid.clone(), dk));
                }
                _ => {}
            }
        }
        Ok(keys)
    }

    /// 解析 Key 内容：支持文件路径或 PEM 内容
    fn resolve_key_content(key_input: &str) -> std::result::Result<String, AuthInfraError> {
        if key_input.starts_with("-----BEGIN") {
            // 直接是 PEM 内容
            Ok(key_input.to_string())
        } else {
            // 视为文件路径
            std::fs::read_to_string(key_input).map_err(|e| {
                AuthInfraError::Auth(cmx_traits::auth::AuthError::InvalidToken(format!(
                    "无法读取密钥文件 {}: {}",
                    key_input, e
                )))
            })
        }
    }

    /// 构建校验规则
    fn validation(&self) -> Validation {
        let mut validation = Validation::new(self.algorithm);
        validation.set_issuer(&[&self.config.jwt.issuer]);
        validation.set_audience(&[&self.config.jwt.audience]);
        validation
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AuthConfig, JwtConfig, TokenExpiryConfig};

    /// 构造用于测试的 `AuthConfig`（HS256 算法 + 自定义密钥）。
    fn make_config(secret: &str, access_ttl: u64, refresh_ttl: u64) -> AuthConfig {
        AuthConfig {
            jwt: JwtConfig {
                algorithm: "HS256".to_string(),
                private_key: None,
                public_key: None,
                secret: Some(secret.to_string()),
                issuer: "cmx-auth-test".to_string(),
                audience: "cmx-platform-test".to_string(),
                current_kid: None,
                legacy_public_keys: vec![],
            },
            token: TokenExpiryConfig {
                access_ttl_secs: access_ttl,
                refresh_ttl_secs: refresh_ttl,
            },
            ..AuthConfig::default()
        }
    }

    /// 构造一个使用默认密钥的 `JwtManager`。
    fn make_manager(secret: &str) -> JwtManager {
        JwtManager::new(make_config(secret, 1800, 604800)).expect("JwtManager 构造失败")
    }

    #[test]
    fn test_jwt_encode_decode_access_token() {
        let mgr = make_manager("test-secret-for-jwt-encode-decode");

        let roles = vec!["admin".to_string(), "user".to_string()];
        let perms = vec!["read".to_string(), "write".to_string()];
        let token = mgr
            .encode_access_token(
                "user-001",
                "alice",
                &roles,
                &perms,
                Some("org-001"),
                "session-001",
                "web",
            )
            .expect("编码 Access Token 失败");

        assert!(!token.is_empty(), "Token 不应为空字符串");
        // JWT 格式应为 header.payload.signature
        assert_eq!(token.split('.').count(), 3, "Token 应为三段式 JWT");

        let claims = mgr
            .decode_access_token(&token)
            .expect("解码 Access Token 失败");

        assert_eq!(claims.sub, "user-001");
        assert_eq!(claims.username, "alice");
        assert_eq!(claims.roles, roles);
        assert_eq!(claims.permissions, perms);
        assert_eq!(claims.org_id.as_deref(), Some("org-001"));
        assert_eq!(claims.sid, "session-001");
        assert_eq!(claims.device, "web");
        assert_eq!(claims.typ, "access");
        assert_eq!(claims.iss, "cmx-auth-test");
        assert_eq!(claims.aud, "cmx-platform-test");
        assert!(!claims.jti.is_empty(), "jti 不应为空");
        assert!(claims.exp > claims.iat, "exp 应大于 iat");
    }

    #[test]
    fn test_jwt_encode_decode_refresh_token() {
        let mgr = make_manager("test-secret-refresh-token");

        let token = mgr
            .encode_refresh_token("user-002", "session-002", "mobile")
            .expect("编码 Refresh Token 失败");

        assert!(!token.is_empty());
        assert_eq!(token.split('.').count(), 3);

        let claims = mgr
            .decode_refresh_token(&token)
            .expect("解码 Refresh Token 失败");

        assert_eq!(claims.sub, "user-002");
        assert_eq!(claims.sid, "session-002");
        assert_eq!(claims.device, "mobile");
        assert_eq!(claims.typ, "refresh");
        assert_eq!(claims.iss, "cmx-auth-test");
        assert!(!claims.jti.is_empty());
        assert!(claims.exp > claims.iat);
    }

    #[test]
    fn test_jwt_expired_access_token_fails() {
        // 手工构造一个已过期的 Access Token（exp 在过去）
        use jsonwebtoken::{encode as jwt_encode, Algorithm, EncodingKey, Header};

        let mgr = make_manager("expired-secret-access");
        let secret = "expired-secret-access";

        let now = Utc::now().timestamp();
        let claims = AccessClaims {
            sub: "user-exp".to_string(),
            exp: now - 3600, // 1 小时前过期
            iat: now - 7200,
            jti: Uuid::new_v4().to_string(),
            iss: "cmx-auth-test".to_string(),
            aud: "cmx-platform-test".to_string(),
            username: "bob".to_string(),
            roles: vec![],
            permissions: vec![],
            org_id: None,
            sid: "session-exp".to_string(),
            device: "web".to_string(),
            typ: "access".to_string(),
            kid: None,
        };

        let header = Header::new(Algorithm::HS256);
        let encoding_key = EncodingKey::from_secret(secret.as_bytes());
        let token = jwt_encode(&header, &claims, &encoding_key).expect("编码过期 Token 失败");

        let result = mgr.decode_access_token(&token);
        assert!(
            result.is_err(),
            "过期 Access Token 应解码失败，实际: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_jwt_expired_refresh_token_fails() {
        use jsonwebtoken::{encode as jwt_encode, Algorithm, EncodingKey, Header};

        let mgr = make_manager("expired-secret-refresh");
        let secret = "expired-secret-refresh";

        let now = Utc::now().timestamp();
        let claims = RefreshClaims {
            sub: "user-exp-r".to_string(),
            exp: now - 3600, // 1 小时前过期
            iat: now - 7200,
            jti: Uuid::new_v4().to_string(),
            iss: "cmx-auth-test".to_string(),
            typ: "refresh".to_string(),
            sid: "session-exp-r".to_string(),
            device: "web".to_string(),
        };

        let header = Header::new(Algorithm::HS256);
        let encoding_key = EncodingKey::from_secret(secret.as_bytes());
        let token = jwt_encode(&header, &claims, &encoding_key).expect("编码过期 Refresh Token 失败");

        let result = mgr.decode_refresh_token(&token);
        assert!(
            result.is_err(),
            "过期 Refresh Token 应解码失败，实际: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_jwt_invalid_signature_access_token_fails() {
        // 用密钥 A 编码，用密钥 B 解码（验签失败）
        let encoder = make_manager("secret-a-for-signing");
        let decoder = make_manager("secret-b-different");

        let token = encoder
            .encode_access_token("user-sig", "carol", &[], &[], None, "session-sig", "web")
            .expect("编码 Access Token 失败");

        let result = decoder.decode_access_token(&token);
        assert!(
            result.is_err(),
            "无效签名的 Access Token 应解码失败，实际: {:?}",
            result
        );
    }

    #[test]
    fn test_jwt_invalid_signature_refresh_token_fails() {
        let encoder = make_manager("secret-a-refresh");
        let decoder = make_manager("secret-b-refresh-diff");

        let token = encoder
            .encode_refresh_token("user-sig-r", "session-sig-r", "web")
            .expect("编码 Refresh Token 失败");

        let result = decoder.decode_refresh_token(&token);
        assert!(
            result.is_err(),
            "无效签名的 Refresh Token 应解码失败，实际: {:?}",
            result
        );
    }

    #[test]
    fn test_jwt_invalid_token_format_fails() {
        let mgr = make_manager("format-test-secret");

        // 非法字符串
        assert!(mgr.decode_access_token("not-a-jwt").is_err());
        assert!(mgr.decode_refresh_token("not-a-jwt").is_err());

        // 空字符串
        assert!(mgr.decode_access_token("").is_err());
        assert!(mgr.decode_refresh_token("").is_err());

        // 三段式但 payload 非法
        assert!(mgr.decode_access_token("aaa.bbb.ccc").is_err());
    }

    #[test]
    fn test_jwt_tampered_payload_fails() {
        let mgr = make_manager("tamper-test-secret");

        let token = mgr
            .encode_access_token("user-tamper", "dave", &[], &[], None, "session-tamper", "web")
            .expect("编码 Access Token 失败");

        // 篡改 signature 部分的最后一个字符（破坏签名）
        let parts: Vec<&str> = token.split('.').collect();
        let sig = parts[2];
        let last_char = sig.chars().last().unwrap();
        let new_last = if last_char == 'A' { 'B' } else { 'A' };
        let tampered_sig = format!("{}{}", &sig[..sig.len() - 1], new_last);
        let tampered_token = format!("{}.{}.{}", parts[0], parts[1], tampered_sig);

        let result = mgr.decode_access_token(&tampered_token);
        assert!(
            result.is_err(),
            "篡改签名的 Token 应解码失败，实际: {:?}",
            result
        );
    }

    #[test]
    fn test_jwt_unsupported_algorithm_fails() {
        let mut config = make_config("any-secret", 1800, 604800);
        config.jwt.algorithm = "ES256".to_string(); // 不支持的算法

        let result = JwtManager::new(config);
        assert!(
            result.is_err(),
            "不支持的算法应导致 JwtManager 构造失败"
        );
    }

    #[test]
    fn test_jwt_unique_jti_per_token() {
        let mgr = make_manager("unique-jti-secret");

        let token1 = mgr
            .encode_access_token("user-u", "eve", &[], &[], None, "session-u", "web")
            .expect("编码 Token 1 失败");
        let token2 = mgr
            .encode_access_token("user-u", "eve", &[], &[], None, "session-u", "web")
            .expect("编码 Token 2 失败");

        let claims1 = mgr.decode_access_token(&token1).expect("解码 Token 1 失败");
        let claims2 = mgr.decode_access_token(&token2).expect("解码 Token 2 失败");

        assert_ne!(
            claims1.jti, claims2.jti,
            "两次编码生成的 jti 应不同（UUID v4）"
        );
    }
}
