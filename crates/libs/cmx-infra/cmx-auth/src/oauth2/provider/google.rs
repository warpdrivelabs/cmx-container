//! Google OAuth2 Provider

use async_trait::async_trait;
use cmx_traits::auth::AuthError;

use super::{OAuth2Provider, ProviderTokenResponse, ProviderUserInfo};
use crate::config::OAuth2ProviderConfig;

/// Google OAuth2 Provider。
///
/// 支持 ID Token 验签（使用 Google JWKS 公钥）和 userinfo endpoint 降级获取用户信息。
pub struct GoogleProvider {
    /// Provider 配置。
    config: OAuth2ProviderConfig,

    /// 异步 HTTP 客户端。
    http_client: reqwest::Client,

    /// Google JWKS 公钥缓存（含过期时间），用于 ID Token 验签。
    jwks: tokio::sync::RwLock<JwksCache>,
}

/// JWKS 缓存条目。
struct JwksCache {
    /// 已缓存的 JWKS JSON。
    keys: Option<serde_json::Value>,

    /// 缓存过期时间点。
    expires_at: Option<std::time::Instant>,
}

impl JwksCache {
    fn new() -> Self {
        Self { keys: None, expires_at: None }
    }

    fn is_valid(&self) -> bool {
        self.keys.is_some()
            && self.expires_at.map_or(false, |t| std::time::Instant::now() < t)
    }

    fn set(&mut self, keys: serde_json::Value, ttl: std::time::Duration) {
        self.keys = Some(keys);
        self.expires_at = Some(std::time::Instant::now() + ttl);
    }

    fn invalidate(&mut self) {
        self.keys = None;
        self.expires_at = None;
    }
}

impl GoogleProvider {
    pub fn new(config: OAuth2ProviderConfig) -> Self {
        Self {
            config,
            http_client: reqwest::Client::new(),
            jwks: tokio::sync::RwLock::new(JwksCache::new()),
        }
    }

    /// Google JWKS endpoint
    const JWKS_URL: &'static str = "https://www.googleapis.com/oauth2/v3/certs";
    /// JWKS 缓存有效期（24 小时）
    const JWKS_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(24 * 3600);

    /// 获取 Google JWKS 公钥（带缓存 + 过期刷新）
    async fn fetch_jwks(&self) -> Result<serde_json::Value, AuthError> {
        {
            let cache = self.jwks.read().await;
            if cache.is_valid() {
                return Ok(cache.keys.as_ref().unwrap().clone());
            }
        }
        tracing::info!("获取 Google JWKS 公钥（缓存过期或首次获取）");
        let resp = self.http_client
            .get(Self::JWKS_URL)
            .send()
            .await
            .map_err(|e| AuthError::OAuth2ProviderUnavailable(e.to_string()))?;

        let jwks: serde_json::Value = resp.json().await
            .map_err(|e| AuthError::OAuth2ProviderUnavailable(e.to_string()))?;

        let mut cache = self.jwks.write().await;
        cache.set(jwks.clone(), Self::JWKS_CACHE_TTL);

        Ok(jwks)
    }

    /// 强制刷新 JWKS 缓存
    async fn force_refresh_jwks(&self) -> Result<serde_json::Value, AuthError> {
        {
            let mut cache = self.jwks.write().await;
            cache.invalidate();
        }
        self.fetch_jwks().await
    }

    /// 验证 ID Token 签名并解析 claims
    async fn verify_id_token(&self, id_token: &str) -> Result<GoogleIdTokenClaims, AuthError> {
        use base64::engine::general_purpose::URL_SAFE_NO_PAD;
        use base64::Engine;

        let parts: Vec<&str> = id_token.split('.').collect();
        if parts.len() != 3 {
            return Err(AuthError::OAuth2ProviderTokenError("ID Token 格式无效".into()));
        }

        let header_json = URL_SAFE_NO_PAD.decode(parts[0])
            .map_err(|e| AuthError::OAuth2ProviderTokenError(format!("Header 解码失败: {}", e)))?;
        let header: serde_json::Value = serde_json::from_slice(&header_json)
            .map_err(|e| AuthError::OAuth2ProviderTokenError(format!("Header 解析失败: {}", e)))?;

        let kid = header["kid"].as_str()
            .ok_or_else(|| AuthError::OAuth2ProviderTokenError("ID Token 缺少 kid".into()))?;

        let jwks = self.fetch_jwks().await?;
        let key = jwks["keys"].as_array()
            .and_then(|keys| keys.iter().find(|k| k["kid"].as_str() == Some(kid)));

        let key = match key {
            Some(k) => k.clone(),
            None => {
                tracing::warn!(kid = %kid, "JWKS 中未找到匹配的 kid，强制刷新 JWKS");
                let refreshed_jwks = self.force_refresh_jwks().await?;
                refreshed_jwks["keys"].as_array()
                    .and_then(|keys| keys.iter().find(|k| k["kid"].as_str() == Some(kid)))
                    .cloned()
                    .ok_or_else(|| AuthError::OAuth2ProviderTokenError("JWKS 中未找到匹配的公钥（刷新后仍无）".into()))?
            }
        };

        let n = key["n"].as_str().ok_or_else(|| AuthError::OAuth2ProviderTokenError("JWKS 公钥缺少 n".into()))?;
        let e = key["e"].as_str().ok_or_else(|| AuthError::OAuth2ProviderTokenError("JWKS 公钥缺少 e".into()))?;

        let decoding_key = jsonwebtoken::DecodingKey::from_rsa_components(n, e)
            .map_err(|e| AuthError::OAuth2ProviderTokenError(format!("公钥构建失败: {}", e)))?;

        let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::RS256);
        validation.set_issuer(&["accounts.google.com", "https://accounts.google.com"]);
        validation.set_audience(&[&self.config.client_id]);

        let claims: GoogleIdTokenClaims = jsonwebtoken::decode(id_token, &decoding_key, &validation)
            .map_err(|e| {
                tracing::warn!(error = %e, "Google ID Token 验证失败");
                AuthError::OAuth2ProviderTokenError(format!("ID Token 验证失败: {}", e))
            })?
            .claims;

        Ok(claims)
    }
}

/// Google ID Token Claims
#[derive(Debug, serde::Deserialize)]
struct GoogleIdTokenClaims {
    sub: String,
    email: Option<String>,
    email_verified: Option<bool>,
    name: Option<String>,
    picture: Option<String>,
}

#[async_trait]
impl OAuth2Provider for GoogleProvider {
    fn name(&self) -> &str { "google" }
    fn display_name(&self) -> &str { "Google" }
    fn icon_url(&self) -> Option<&str> { Some("https://www.gstatic.com/firebasejs/ui/identity/google.svg") }
    fn brand_color(&self) -> Option<&str> { Some("#4285F4") }

    fn build_authorize_url(&self, state: &str, redirect_uri: &str, scopes: &[String]) -> String {
        let scopes_str = if scopes.is_empty() {
            self.config.scopes.join(" ")
        } else {
            scopes.join(" ")
        };
        format!(
            "https://accounts.google.com/o/oauth2/v2/auth?response_type=code&client_id={}&redirect_uri={}&scope={}&state={}&access_type=offline&prompt=consent",
            self.config.client_id,
            urlencoding::encode(redirect_uri),
            urlencoding::encode(&scopes_str),
            state,
        )
    }

    async fn exchange_code(&self, code: &str, redirect_uri: &str) -> Result<ProviderTokenResponse, AuthError> {
        tracing::info!(provider = "google", "向 Google 交换 Token");

        let resp = self.http_client
            .post("https://oauth2.googleapis.com/token")
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", code),
                ("client_id", &self.config.client_id),
                ("client_secret", &self.config.client_secret),
                ("redirect_uri", redirect_uri),
            ])
            .send()
            .await
            .map_err(|e| {
                tracing::warn!(provider = "google", error = %e, "Google Token 端点不可达");
                AuthError::OAuth2ProviderUnavailable(e.to_string())
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(AuthError::OAuth2ProviderTokenError(format!(
                "HTTP {}: {}", status, body
            )));
        }

        resp.json::<ProviderTokenResponse>().await
            .map_err(|e| AuthError::OAuth2ProviderTokenError(e.to_string()))
    }

    async fn get_user_info(&self, token_response: &ProviderTokenResponse) -> Result<ProviderUserInfo, AuthError> {
        if let Some(id_token) = &token_response.id_token {
            match self.verify_id_token(id_token).await {
                Ok(claims) => {
                    tracing::info!(provider = "google", sub = %claims.sub, "ID Token 验证成功，解析用户信息");
                    return Ok(ProviderUserInfo {
                        provider_user_id: claims.sub,
                        email: claims.email,
                        email_verified: claims.email_verified,
                        username: None,
                        display_name: claims.name,
                        avatar_url: claims.picture,
                    });
                }
                Err(e) => {
                    tracing::warn!(provider = "google", error = %e, "ID Token 验证失败，降级到 userinfo endpoint");
                }
            }
        }

        let resp = self.http_client
            .get("https://www.googleapis.com/oauth2/v3/userinfo")
            .bearer_auth(&token_response.access_token)
            .send()
            .await
            .map_err(|e| AuthError::OAuth2ProviderUnavailable(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(AuthError::OAuth2ProviderUserInfoError(
                format!("HTTP {}", resp.status())
            ));
        }

        let json: serde_json::Value = resp.json().await
            .map_err(|e| AuthError::OAuth2ProviderUserInfoError(e.to_string()))?;

        Ok(ProviderUserInfo {
            provider_user_id: json["sub"].as_str().unwrap_or("").to_string(),
            email: json["email"].as_str().map(String::from),
            email_verified: json["email_verified"].as_bool(),
            username: None,
            display_name: json["name"].as_str().map(String::from),
            avatar_url: json["picture"].as_str().map(String::from),
        })
    }

    fn default_scopes(&self) -> Vec<String> {
        vec!["openid".into(), "email".into(), "profile".into()]
    }

    fn redirect_uri(&self) -> &str {
        &self.config.redirect_uri
    }
}
