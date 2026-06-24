//! 通用 OAuth2 Provider（配置驱动）

use async_trait::async_trait;
use cmx_traits::auth::AuthError;

use super::{OAuth2Provider, ProviderTokenResponse, ProviderUserInfo};
use crate::config::OAuth2ProviderConfig;

/// 用于 `navigate_path` 路径未找到时的 fallback，避免返回临时值的引用。
const JSON_NULL: serde_json::Value = serde_json::Value::Null;

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
        let http_client = if config.skip_ssl_verification {
            reqwest::Client::builder()
                .danger_accept_invalid_certs(true)
                .build()
                .unwrap_or_else(|_| reqwest::Client::new())
        } else {
            reqwest::Client::new()
        };
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

        // 使用 `Vec<(String, String)>` 而非 `Vec<(&str, String)>`，
        // 避免 `authorize_extra_params` 借用导致的生命周期冲突。
        let mut params: Vec<(String, String)> = vec![
            ("response_type".to_string(), "code".to_string()),
            ("client_id".to_string(), self.config.client_id.clone()),
            ("redirect_uri".to_string(), redirect_uri.to_string()),
            ("state".to_string(), state.to_string()),
        ];

        // scope 非空时才添加，部分厂商拒绝空 scope
        if !scopes_str.is_empty() {
            params.push(("scope".to_string(), scopes_str));
        }

        // 追加授权 URL 额外参数（如 Azure AD `resource`）
        for (k, v) in &self.config.authorize_extra_params {
            params.push((k.clone(), v.clone()));
        }

        let query = params.iter()
            .map(|(k, v)| format!("{}={}", k, urlencoding::encode(v)))
            .collect::<Vec<_>>()
            .join("&");

        format!("{}?{}", self.config.authorize_url, query)
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
            tracing::warn!(provider = %self.name(), status = %status, body = %body, "Token 交换失败");
            return Err(AuthError::OAuth2ProviderTokenError(format!(
                "HTTP {}: {}", status, body
            )));
        }

        // 先解析为 JSON Value，支持嵌套路径导航与字段映射
        let json: serde_json::Value = resp.json().await
            .map_err(|e| {
                tracing::warn!(provider = %self.name(), error = %e, "Token 响应解析失败");
                AuthError::OAuth2ProviderTokenError(e.to_string())
            })?;

        tracing::info!(provider = %self.name(), response = %json, "Token 端点原始响应");

        // 导航到嵌套路径（如企业 CAS 的 `{"code":0,"data":{...}}`）
        let token_json = Self::navigate_path(&json, &self.config.token_response_path);

        let mapping = &self.config.token_field_mapping;
        let access_token = Self::extract_string(token_json, mapping, "access_token");
        if access_token.is_empty() {
            tracing::warn!(provider = %self.name(), "Token 响应中缺少 access_token 字段");
            return Err(AuthError::OAuth2ProviderTokenError(
                "Token 响应中缺少 access_token 字段".to_string()
            ));
        }

        Ok(ProviderTokenResponse {
            access_token,
            token_type: Self::extract_string_or_default(token_json, mapping, "token_type", "bearer"),
            expires_in: Self::extract_u64_opt(token_json, mapping, "expires_in"),
            refresh_token: Self::extract_string_opt(token_json, mapping, "refresh_token"),
            scope: Self::extract_string_opt(token_json, mapping, "scope"),
            id_token: Self::extract_string_opt(token_json, mapping, "id_token"),
        })
    }

    async fn get_user_info(
        &self,
        token_response: &ProviderTokenResponse,
    ) -> Result<ProviderUserInfo, AuthError> {
        tracing::info!(provider = %self.name(), "获取第三方 Provider 用户信息");

        let method_is_get = self.config.userinfo_method.to_uppercase() != "POST";

        // 1. 构建请求（GET/POST）
        let mut req = if method_is_get {
            self.http_client.get(&self.config.userinfo_url)
        } else {
            self.http_client.post(&self.config.userinfo_url)
        };

        // 2. 追加额外参数（始终作为 query，GET/POST 均同，与 Java 实现一致）
        if !self.config.userinfo_extra_params.is_empty() {
            let params: Vec<(&str, &str)> = self.config.userinfo_extra_params
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect();
            req = req.query(&params);
        }

        // 3. token 传递方式（GET + form 不合理，自动降级为 query）
        let token_param = self.config.userinfo_token_param.as_str();
        req = match token_param {
            "query" => req.query(&[("access_token", &token_response.access_token)]),
            "form" if !method_is_get => {
                req.form(&[("access_token", &token_response.access_token)])
            }
            "form" if method_is_get => {
                tracing::warn!(provider = %self.name(), "GET 方法不支持 form 传参，降级为 query");
                req.query(&[("access_token", &token_response.access_token)])
            }
            _ => req.bearer_auth(&token_response.access_token),
        };

        // 4. 发送请求
        let resp = req.send().await
            .map_err(|e| {
                tracing::warn!(provider = %self.name(), error = %e, "用户信息请求失败");
                AuthError::OAuth2ProviderUnavailable(e.to_string())
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            tracing::warn!(provider = %self.name(), status = %status, body = %body, "用户信息请求失败");
            return Err(AuthError::OAuth2ProviderUserInfoError(
                format!("HTTP {}: {}", status, body)
            ));
        }

        // 5. 解析响应
        let json: serde_json::Value = resp.json().await
            .map_err(|e| AuthError::OAuth2ProviderUserInfoError(e.to_string()))?;

        tracing::info!(provider = %self.name(), response = %json, "用户信息端点原始响应");

        // 6. 导航到嵌套路径
        let user_json = Self::navigate_path(&json, &self.config.userinfo_response_path);

        // 7. 字段映射提取
        let mapping = &self.config.field_mapping;
        let provider_user_id = Self::extract_string(user_json, mapping, "provider_user_id");
        if provider_user_id.is_empty() {
            tracing::warn!(provider = %self.name(), "用户信息响应中缺少 provider_user_id 字段");
            return Err(AuthError::OAuth2ProviderUserInfoError(
                "用户信息响应中缺少 provider_user_id 字段".to_string()
            ));
        }

        Ok(ProviderUserInfo {
            provider_user_id,
            email: Self::extract_string_opt(user_json, mapping, "email"),
            email_verified: Self::extract_bool_opt(user_json, mapping, "email_verified"),
            username: Self::extract_string_opt(user_json, mapping, "username"),
            display_name: Self::extract_string_opt(user_json, mapping, "display_name"),
            avatar_url: Self::extract_string_opt(user_json, mapping, "avatar_url"),
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
    /// 按点分路径导航 JSON（如 `data` 或 `result.data`），空路径返回根对象。
    ///
    /// 路径未找到时返回 `&JSON_NULL`（const 常量），避免临时值生命周期问题。
    fn navigate_path<'a>(json: &'a serde_json::Value, path: &str) -> &'a serde_json::Value {
        if path.is_empty() {
            return json;
        }
        let mut current = json;
        for segment in path.split('.') {
            current = current.get(segment).unwrap_or(&JSON_NULL);
        }
        current
    }

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

    /// 提取字符串字段，缺失时返回默认值
    fn extract_string_or_default(json: &serde_json::Value, mapping: &std::collections::HashMap<String, String>, field: &str, default: &str) -> String {
        Self::extract_string_opt(json, mapping, field).unwrap_or_else(|| default.to_string())
    }

    /// 从 JSON 中提取可选布尔字段，兼容 boolean / 字符串 / 数字
    ///
    /// 部分厂商 `email_verified` 返回 `"true"` 字符串或 `1` 数字。
    fn extract_bool_opt(json: &serde_json::Value, mapping: &std::collections::HashMap<String, String>, field: &str) -> Option<bool> {
        let json_key = mapping.get(field).map(|s| s.as_str()).unwrap_or(field);
        json.get(json_key).and_then(|v| {
            match v {
                serde_json::Value::Bool(b) => Some(*b),
                serde_json::Value::String(s) => match s.to_lowercase().as_str() {
                    "true" | "1" | "yes" => Some(true),
                    "false" | "0" | "no" => Some(false),
                    _ => None,
                },
                serde_json::Value::Number(n) => n.as_i64().map(|i| i != 0),
                _ => None,
            }
        })
    }

    /// 从 JSON 中提取可选 u64 字段，兼容数字和字符串数字
    ///
    /// 部分厂商 `expires_in` 返回 `"3600"` 字符串而非数字。
    fn extract_u64_opt(json: &serde_json::Value, mapping: &std::collections::HashMap<String, String>, field: &str) -> Option<u64> {
        let json_key = mapping.get(field).map(|s| s.as_str()).unwrap_or(field);
        json.get(json_key).and_then(|v| {
            match v {
                serde_json::Value::Number(n) => n.as_u64(),
                serde_json::Value::String(s) => s.parse::<u64>().ok(),
                _ => None,
            }
        })
    }
}
