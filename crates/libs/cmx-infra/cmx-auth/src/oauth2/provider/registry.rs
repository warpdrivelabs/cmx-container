//! OAuth2 Provider 注册表。
//!
//! 提供全局 Provider 注册表，按 `name` 索引查找已注册的 `OAuth2Provider` 实现。

use std::collections::HashMap;
use std::sync::Arc;

use cmx_traits::auth::AuthError;

use super::OAuth2Provider;

/// 全局 OAuth2 Provider 注册表（`cmx-auth` 内部使用）。
static GLOBAL_PROVIDER_REGISTRY: std::sync::OnceLock<OAuth2ProviderRegistry> =
    std::sync::OnceLock::new();

/// Provider 注册表。
///
/// 持有所有已注册的 `OAuth2Provider` 实现，按 `name` 索引查找。
#[derive(Clone)]
pub struct OAuth2ProviderRegistry {
    /// 已注册的 Provider 集合：`name` → `Arc<dyn OAuth2Provider>`。
    providers: HashMap<String, Arc<dyn OAuth2Provider>>,
}

impl OAuth2ProviderRegistry {
    /// 创建空的注册表。
    ///
    /// # Returns
    ///
    /// 返回不含任何 Provider 的空 `OAuth2ProviderRegistry` 实例。
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
        }
    }

    /// 注册 Provider。
    ///
    /// # Arguments
    ///
    /// * `provider` - 待注册的 `OAuth2Provider` trait 对象。
    pub fn register(&mut self, provider: Arc<dyn OAuth2Provider>) {
        tracing::info!(provider = %provider.name(), display_name = %provider.display_name(), "注册 OAuth2 Provider");
        self.providers.insert(provider.name().to_string(), provider);
    }

    /// 获取指定名称的 Provider。
    ///
    /// # Arguments
    ///
    /// * `name` - Provider 名称（如 `google`、`github`）。
    ///
    /// # Returns
    ///
    /// 存在时返回 `Ok(Arc<dyn OAuth2Provider>)`，否则返回 `AuthError::OAuth2ProviderNotFound`。
    pub fn get_provider(&self, name: &str) -> Result<Arc<dyn OAuth2Provider>, AuthError> {
        self.providers
            .get(name)
            .cloned()
            .ok_or_else(|| AuthError::OAuth2ProviderNotFound(name.to_string()))
    }

    /// 列出所有已注册的 Provider 信息。
    ///
    /// # Returns
    ///
    /// 返回所有 Provider 的 `ProviderInfo` 列表（含名称、显示名、scope、图标、品牌色）。
    pub fn list_providers(&self) -> Vec<cmx_traits::auth::ProviderInfo> {
        self.providers
            .values()
            .map(|p| cmx_traits::auth::ProviderInfo {
                name: p.name().to_string(),
                display_name: p.display_name().to_string(),
                scopes: p.default_scopes(),
                icon_url: p.icon_url().map(String::from),
                brand_color: p.brand_color().map(String::from),
            })
            .collect()
    }
}

impl Default for OAuth2ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl OAuth2ProviderRegistry {
    /// 初始化全局 Provider 注册表。
    ///
    /// 使用 `OnceLock` 保证全局只可初始化一次，重复初始化将返回错误。
    ///
    /// # Arguments
    ///
    /// * `registry` - 待注册到全局的 `OAuth2ProviderRegistry` 实例。
    ///
    /// # Returns
    ///
    /// 成功时返回 `Ok(())`，已初始化时返回 `Err(String)`。
    pub fn initialize_global(registry: OAuth2ProviderRegistry) -> Result<(), String> {
        GLOBAL_PROVIDER_REGISTRY
            .set(registry)
            .map_err(|_| "OAuth2 Provider 注册表已初始化".to_string())
    }

    /// 获取全局 Provider 注册表。
    ///
    /// # Returns
    ///
    /// 已初始化时返回 `Some(&'static OAuth2ProviderRegistry)`，未初始化时返回 `None`。
    pub fn get_global() -> Option<&'static OAuth2ProviderRegistry> {
        GLOBAL_PROVIDER_REGISTRY.get()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::Arc;
    // ProviderTokenResponse / ProviderUserInfo 定义在父模块 provider/mod.rs，
    // `super::*` 只能引入 registry 模块自身内容，需显式导入。
    use crate::oauth2::provider::{ProviderTokenResponse, ProviderUserInfo};

    /// 用于测试的 Mock Provider。
    struct MockProvider {
        name: String,
        display_name: String,
        scopes: Vec<String>,
    }

    #[async_trait]
    impl OAuth2Provider for MockProvider {
        fn name(&self) -> &str {
            &self.name
        }
        fn display_name(&self) -> &str {
            &self.display_name
        }
        fn icon_url(&self) -> Option<&str> {
            Some("https://example.com/icon.png")
        }
        fn brand_color(&self) -> Option<&str> {
            Some("#FF0000")
        }
        fn build_authorize_url(
            &self,
            state: &str,
            redirect_uri: &str,
            scopes: &[String],
        ) -> String {
            format!(
                "https://mock.example.com/authorize?state={}&redirect_uri={}&scopes={}",
                state,
                redirect_uri,
                scopes.join(",")
            )
        }
        async fn exchange_code(
            &self,
            _code: &str,
            _redirect_uri: &str,
        ) -> Result<ProviderTokenResponse, AuthError> {
            Ok(ProviderTokenResponse {
                access_token: "mock-token".to_string(),
                token_type: "bearer".to_string(),
                expires_in: Some(3600),
                refresh_token: None,
                scope: None,
                id_token: None,
            })
        }
        async fn get_user_info(
            &self,
            _token_response: &ProviderTokenResponse,
        ) -> Result<ProviderUserInfo, AuthError> {
            Ok(ProviderUserInfo {
                provider_user_id: "mock-uid".to_string(),
                email: Some("mock@example.com".to_string()),
                email_verified: Some(true),
                username: Some("mockuser".to_string()),
                display_name: Some("Mock User".to_string()),
                avatar_url: None,
            })
        }
        fn default_scopes(&self) -> Vec<String> {
            self.scopes.clone()
        }
        fn redirect_uri(&self) -> &str {
            "https://app.example.com/callback"
        }
    }

    fn make_mock(name: &str, display: &str, scopes: Vec<String>) -> Arc<dyn OAuth2Provider> {
        Arc::new(MockProvider {
            name: name.to_string(),
            display_name: display.to_string(),
            scopes,
        })
    }

    #[test]
    fn test_provider_registry_register_and_get() {
        let mut registry = OAuth2ProviderRegistry::new();
        let provider = make_mock("google", "Google", vec!["openid".into(), "email".into()]);
        registry.register(provider);

        let got = registry
            .get_provider("google")
            .expect("已注册的 Provider 应能获取到");
        assert_eq!(got.name(), "google");
        assert_eq!(got.display_name(), "Google");
        assert_eq!(got.default_scopes(), vec!["openid", "email"]);
        assert_eq!(got.icon_url(), Some("https://example.com/icon.png"));
        assert_eq!(got.brand_color(), Some("#FF0000"));
    }

    #[test]
    fn test_provider_registry_get_nonexistent_returns_error() {
        let registry = OAuth2ProviderRegistry::new();

        let result = registry.get_provider("nonexistent");
        assert!(result.is_err(), "获取未注册的 Provider 应返回错误");
        match result {
            Err(AuthError::OAuth2ProviderNotFound(name)) => {
                assert_eq!(name, "nonexistent");
            }
            _ => panic!("期望 OAuth2ProviderNotFound 错误"),
        }
    }

    #[test]
    fn test_provider_registry_list_providers() {
        let mut registry = OAuth2ProviderRegistry::new();
        registry.register(make_mock("google", "Google", vec!["openid".into()]));
        registry.register(make_mock("github", "GitHub", vec!["repo".into()]));

        let list = registry.list_providers();
        assert_eq!(list.len(), 2, "应列出 2 个 Provider");

        // 不依赖顺序查找（HashMap 顺序未定）
        let names: Vec<String> = list.iter().map(|p| p.name.clone()).collect();
        assert!(names.contains(&"google".to_string()), "应包含 google");
        assert!(names.contains(&"github".to_string()), "应包含 github");

        // 验证 ProviderInfo 字段映射
        let google = list
            .iter()
            .find(|p| p.name == "google")
            .expect("应找到 google");
        assert_eq!(google.display_name, "Google");
        assert_eq!(google.scopes, vec!["openid"]);
        assert_eq!(
            google.icon_url.as_deref(),
            Some("https://example.com/icon.png")
        );
        assert_eq!(google.brand_color.as_deref(), Some("#FF0000"));
    }

    #[test]
    fn test_provider_registry_replace_existing() {
        let mut registry = OAuth2ProviderRegistry::new();

        // 注册 google v1
        registry.register(make_mock("google", "Google v1", vec!["old".into()]));

        // 用同名但不同显示名和 scope 替换
        registry.register(make_mock("google", "Google v2", vec!["new".into()]));

        // 应只保留最新注册的
        let list = registry.list_providers();
        assert_eq!(list.len(), 1, "同名注册应替换而非追加");

        let got = registry.get_provider("google").expect("google 应存在");
        assert_eq!(got.display_name(), "Google v2", "替换后应使用最新版本");
        assert_eq!(got.default_scopes(), vec!["new"]);
    }

    #[test]
    fn test_provider_registry_empty() {
        let registry = OAuth2ProviderRegistry::new();
        assert!(registry.list_providers().is_empty(), "空注册表应返回空列表");
    }

    #[test]
    fn test_provider_registry_default_is_empty() {
        let registry = OAuth2ProviderRegistry::default();
        assert!(registry.list_providers().is_empty());
    }
}
