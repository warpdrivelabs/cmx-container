//! OAuth2 Provider 注册表

use std::collections::HashMap;
use std::sync::Arc;

use cmx_traits::auth::AuthError;

use super::OAuth2Provider;

/// 全局 OAuth2 Provider 注册表（cmx-auth 内部使用）
static GLOBAL_PROVIDER_REGISTRY: std::sync::OnceLock<OAuth2ProviderRegistry> = std::sync::OnceLock::new();

/// Provider 注册表
#[derive(Clone)]
pub struct OAuth2ProviderRegistry {
    providers: HashMap<String, Arc<dyn OAuth2Provider>>,
}

impl OAuth2ProviderRegistry {
    /// 创建空的注册表
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
        }
    }

    /// 注册 Provider
    pub fn register(&mut self, provider: Arc<dyn OAuth2Provider>) {
        tracing::info!(provider = %provider.name(), display_name = %provider.display_name(), "注册 OAuth2 Provider");
        self.providers.insert(provider.name().to_string(), provider);
    }

    /// 获取 Provider
    pub fn get_provider(&self, name: &str) -> Result<Arc<dyn OAuth2Provider>, AuthError> {
        self.providers.get(name).cloned().ok_or_else(|| {
            AuthError::OAuth2ProviderNotFound(name.to_string())
        })
    }

    /// 列出所有已注册的 Provider 信息
    pub fn list_providers(&self) -> Vec<cmx_traits::auth::ProviderInfo> {
        self.providers.values().map(|p| cmx_traits::auth::ProviderInfo {
            name: p.name().to_string(),
            display_name: p.display_name().to_string(),
            scopes: p.default_scopes(),
            icon_url: p.icon_url().map(String::from),
            brand_color: p.brand_color().map(String::from),
        }).collect()
    }
}

impl Default for OAuth2ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl OAuth2ProviderRegistry {
    /// 初始化全局 Provider 注册表
    pub fn initialize_global(registry: OAuth2ProviderRegistry) -> Result<(), String> {
        GLOBAL_PROVIDER_REGISTRY
            .set(registry)
            .map_err(|_| "OAuth2 Provider 注册表已初始化".to_string())
    }

    /// 获取全局 Provider 注册表
    pub fn get_global() -> Option<&'static OAuth2ProviderRegistry> {
        GLOBAL_PROVIDER_REGISTRY.get()
    }
}
