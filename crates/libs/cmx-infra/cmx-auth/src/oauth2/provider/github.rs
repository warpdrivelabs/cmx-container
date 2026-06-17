//! GitHub OAuth2 Provider

use async_trait::async_trait;
use cmx_traits::auth::AuthError;

use super::{OAuth2Provider, ProviderTokenResponse, ProviderUserInfo};
use crate::config::OAuth2ProviderConfig;

/// GitHub OAuth2 Provider
pub struct GitHubProvider {
    config: OAuth2ProviderConfig,
    http_client: reqwest::Client,
}

impl GitHubProvider {
    pub fn new(config: OAuth2ProviderConfig) -> Self {
        Self { config, http_client: reqwest::Client::new() }
    }
}

#[async_trait]
impl OAuth2Provider for GitHubProvider {
    fn name(&self) -> &str { "github" }
    fn display_name(&self) -> &str { "GitHub" }
    fn icon_url(&self) -> Option<&str> { Some("https://github.githubassets.com/favicons/favicon-dark.svg") }
    fn brand_color(&self) -> Option<&str> { Some("#24292e") }

    fn build_authorize_url(&self, state: &str, redirect_uri: &str, scopes: &[String]) -> String {
        let scopes_str = if scopes.is_empty() {
            self.config.scopes.join(" ")
        } else {
            scopes.join(" ")
        };
        format!(
            "https://github.com/login/oauth/authorize?client_id={}&redirect_uri={}&scope={}&state={}",
            self.config.client_id,
            urlencoding::encode(redirect_uri),
            urlencoding::encode(&scopes_str),
            state,
        )
    }

    async fn exchange_code(&self, code: &str, redirect_uri: &str) -> Result<ProviderTokenResponse, AuthError> {
        tracing::info!(provider = "github", "向 GitHub 交换 Token");

        let resp = self.http_client
            .post("https://github.com/login/oauth/access_token")
            .header("Accept", "application/json")
            .form(&[
                ("client_id", self.config.client_id.as_str()),
                ("client_secret", self.config.client_secret.as_str()),
                ("code", code),
                ("redirect_uri", redirect_uri),
            ])
            .send()
            .await
            .map_err(|e| {
                tracing::warn!(provider = "github", error = %e, "GitHub Token 端点不可达");
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
        tracing::info!(provider = "github", "获取 GitHub 用户信息");

        let resp = self.http_client
            .get("https://api.github.com/user")
            .bearer_auth(&token_response.access_token)
            .header("User-Agent", "cmx-auth")
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

        let provider_user_id = json["id"].as_i64().map(|n| n.to_string()).unwrap_or_default();
        let email = json["email"].as_str().map(String::from);
        let username = json["login"].as_str().map(String::from);
        let display_name = json["name"].as_str().map(String::from);
        let avatar_url = json["avatar_url"].as_str().map(String::from);

        let email_verified = if email.is_some() {
            self.fetch_github_email_verified(&token_response.access_token, &email).await
        } else {
            None
        };

        Ok(ProviderUserInfo {
            provider_user_id,
            email,
            email_verified,
            username,
            display_name,
            avatar_url,
        })
    }

    fn default_scopes(&self) -> Vec<String> {
        vec!["user:email".into(), "read:user".into()]
    }

    fn redirect_uri(&self) -> &str {
        &self.config.redirect_uri
    }
}

impl GitHubProvider {
    /// 调用 GitHub /user/emails API 获取邮箱验证状态
    async fn fetch_github_email_verified(
        &self,
        access_token: &str,
        primary_email: &Option<String>,
    ) -> Option<bool> {
        let resp = self.http_client
            .get("https://api.github.com/user/emails")
            .bearer_auth(access_token)
            .header("User-Agent", "cmx-auth")
            .send()
            .await;

        match resp {
            Ok(resp) if resp.status().is_success() => {
                match resp.json::<Vec<serde_json::Value>>().await {
                    Ok(emails) => {
                        if let Some(primary) = primary_email {
                            for e in &emails {
                                if e["email"].as_str() == Some(primary.as_str()) {
                                    return e["verified"].as_bool();
                                }
                            }
                        }
                        for e in &emails {
                            if e["primary"].as_bool() == Some(true) {
                                return e["verified"].as_bool();
                            }
                        }
                        None
                    }
                    Err(e) => {
                        tracing::warn!(provider = "github", error = %e, "GitHub emails 响应解析失败");
                        None
                    }
                }
            }
            Ok(resp) => {
                tracing::warn!(provider = "github", status = %resp.status(), "GitHub /user/emails 请求失败");
                None
            }
            Err(e) => {
                tracing::warn!(provider = "github", error = %e, "GitHub /user/emails 请求失败");
                None
            }
        }
    }
}
