//! 通用 OAuth2 Provider（配置驱动）

use async_trait::async_trait;
use cmx_traits::auth::AuthError;

use super::{OAuth2Provider, ProviderTokenResponse, ProviderUserInfo};
use crate::config::OAuth2ProviderConfig;

/// 通用 OAuth2 Provider（配置驱动）。
///
/// 不硬编码特定 Provider 的端点 URL，所有信息来自 `OAuth2ProviderConfig`，
/// 适用于任意符合 OAuth2 标准的 Provider。
pub struct GenericOAuth2Provider {
    /// Provider 配置（含 client_id、client_secret、端点 URL 等）。
    config: OAuth2ProviderConfig,

    /// 异步 HTTP 客户端。
    http_client: reqwest::Client,
}

impl GenericOAuth2Provider {
    pub fn new(config: OAuth2ProviderConfig) -> Self {
        let http_client = reqwest::Client::new();
        Self { config, http_client }
    }
}

#[async_trait]
impl OAuth2Provider for GenericOAuth2Provider {
    fn name(&self) -> &str { &self.config.name }
    fn display_name(&self) -> &str { &self.config.display_name }
    fn icon_url(&self) -> Option<&str> { self.config.icon_url.as_deref() }
    fn brand_color(&self) -> Option<&str> { self.config.brand_color.as_deref() }

    fn build_authorize_url(&self, state: &str, redirect_uri: &str, scopes: &[String]) -> String {
        let scopes_str = if scopes.is_empty() {
            self.config.scopes.join(" ")
        } else {
            scopes.join(" ")
        };
        format!(
            "{}?response_type=code&client_id={}&redirect_uri={}&scope={}&state={}",
            self.config.authorize_url,
            self.config.client_id,
            urlencoding::encode(redirect_uri),
            urlencoding::encode(&scopes_str),
            state,
        )
    }

    async fn exchange_code(
        &self,
        code: &str,
        redirect_uri: &str,
    ) -> Result<ProviderTokenResponse, AuthError> {
        tracing::info!(provider = %self.name(), "向第三方 Provider 交换 Token");

        let mut req = self.http_client
            .post(&self.config.token_url);

        req = match self.config.token_endpoint_auth_method.as_str() {
            "client_secret_basic" => {
                req.basic_auth(&self.config.client_id, Some(&self.config.client_secret))
                    .form(&[
                        ("grant_type", "authorization_code"),
                        ("code", code),
                        ("redirect_uri", redirect_uri),
                    ])
            }
            _ => {
                req.form(&[
                    ("grant_type", "authorization_code"),
                    ("code", code),
                    ("client_id", &self.config.client_id),
                    ("client_secret", &self.config.client_secret),
                    ("redirect_uri", redirect_uri),
                ])
            }
        };

        let resp = req.send().await
            .map_err(|e| {
                tracing::warn!(provider = %self.name(), error = %e, "Provider 服务不可达");
                AuthError::OAuth2ProviderUnavailable(e.to_string())
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            tracing::warn!(provider = %self.name(), status = %status, "Token 交换失败");
            return Err(AuthError::OAuth2ProviderTokenError(format!(
                "HTTP {}: {}", status, body
            )));
        }

        resp.json::<ProviderTokenResponse>().await
            .map_err(|e| {
                tracing::warn!(provider = %self.name(), error = %e, "Token 响应解析失败");
                AuthError::OAuth2ProviderTokenError(e.to_string())
            })
    }

    async fn get_user_info(
        &self,
        token_response: &ProviderTokenResponse,
    ) -> Result<ProviderUserInfo, AuthError> {
        tracing::info!(provider = %self.name(), "获取第三方 Provider 用户信息");

        let resp = self.http_client
            .get(&self.config.userinfo_url)
            .bearer_auth(&token_response.access_token)
            .send()
            .await
            .map_err(|e| {
                tracing::warn!(provider = %self.name(), error = %e, "用户信息请求失败");
                AuthError::OAuth2ProviderUnavailable(e.to_string())
            })?;

        if !resp.status().is_success() {
            return Err(AuthError::OAuth2ProviderUserInfoError(
                format!("HTTP {}", resp.status())
            ));
        }

        let json: serde_json::Value = resp.json().await
            .map_err(|e| AuthError::OAuth2ProviderUserInfoError(e.to_string()))?;

        let mapping = &self.config.field_mapping;
        Ok(ProviderUserInfo {
            provider_user_id: Self::extract_string(&json, mapping, "provider_user_id"),
            email: Self::extract_string_opt(&json, mapping, "email"),
            email_verified: Self::extract_bool_opt(&json, mapping, "email_verified"),
            username: Self::extract_string_opt(&json, mapping, "username"),
            display_name: Self::extract_string_opt(&json, mapping, "display_name"),
            avatar_url: Self::extract_string_opt(&json, mapping, "avatar_url"),
        })
    }

    fn default_scopes(&self) -> Vec<String> {
        self.config.scopes.clone()
    }

    fn redirect_uri(&self) -> &str {
        &self.config.redirect_uri
    }
}

impl GenericOAuth2Provider {
    /// 从 JSON 中提取字符串字段，支持 number→string 自动转换
    fn extract_string(json: &serde_json::Value, mapping: &std::collections::HashMap<String, String>, field: &str) -> String {
        let json_key = mapping.get(field).map(|s| s.as_str()).unwrap_or(field);
        match json.get(json_key) {
            Some(v) => match v {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Number(n) => n.to_string(),
                _ => String::new(),
            },
            None => String::new(),
        }
    }

    /// 从 JSON 中提取可选字符串字段
    fn extract_string_opt(json: &serde_json::Value, mapping: &std::collections::HashMap<String, String>, field: &str) -> Option<String> {
        let json_key = mapping.get(field).map(|s| s.as_str()).unwrap_or(field);
        json.get(json_key).and_then(|v| match v {
            serde_json::Value::String(s) => Some(s.clone()),
            serde_json::Value::Number(n) => Some(n.to_string()),
            _ => None,
        })
    }

    /// 从 JSON 中提取可选布尔字段
    fn extract_bool_opt(json: &serde_json::Value, mapping: &std::collections::HashMap<String, String>, field: &str) -> Option<bool> {
        let json_key = mapping.get(field).map(|s| s.as_str()).unwrap_or(field);
        json.get(json_key).and_then(|v| v.as_bool())
    }
}
