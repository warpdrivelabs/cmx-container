//! JWT 编解码器

use chrono::Utc;
use jsonwebtoken::{decode, decode_header, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use uuid::Uuid;

use crate::config::AuthConfig;
use crate::error::{AuthInfraError, Result};
use super::claims::{AccessClaims, RefreshClaims};

/// JWT 管理器。
///
/// 负责 JWT Token 的编码与解码，支持 RS256 / HS256 算法与密钥轮换宽限期。
/// 解码时优先使用当前密钥，失败后回退到 `legacy_public_keys` 列表。
pub struct JwtManager {
    /// 认证配置。
    config: AuthConfig,

    /// 当前签名密钥。
    encoding_key: EncodingKey,

    /// 当前验签密钥。
    decoding_key: DecodingKey,

    /// 签名算法（RS256 / HS256）。
    algorithm: Algorithm,

    /// 旧密钥列表（`kid` → `DecodingKey`），用于密钥轮换宽限期内的 Token 验签。
    legacy_decoding_keys: Vec<(String, DecodingKey)>,
}

impl JwtManager {
    /// 创建新的 JwtManager
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

    /// 编码 Access Token
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

    /// 编码 Refresh Token
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

    /// 解码 Access Token
    pub fn decode_access_token(&self, token: &str) -> Result<AccessClaims> {
        self.decode_with_key_fallback(token)
    }

    /// 解码 Refresh Token
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
                if legacy_kid == kid {
                    if let Ok(token_data) = decode::<T>(token, legacy_key, &self.validation()) {
                        return Ok(token_data.claims);
                    }
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
                    .unwrap_or("change-me-in-production");
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
