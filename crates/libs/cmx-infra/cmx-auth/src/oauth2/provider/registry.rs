//! OAuth2 Provider 注册表。
//!
//! 提供全局 Provider 注册表，按 `name` 索引查找已注册的 `OAuth2Provider` 实现。

use std::collections::HashMap;
use std::sync::Arc;

use cmx_traits::auth::AuthError;

use super::OAuth2Provider;

/// 全局 OAuth2 Provider 注册表（`cmx-auth` 内部使用）。
static GLOBAL_PROVIDER_REGISTRY: std::sync::OnceLock<OAuth2ProviderRegistry> = std::sync::OnceLock::new();

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
        self.providers.get(name).cloned().ok_or_else(|| {
            AuthError::OAuth2ProviderNotFound(name.to_string())
        })
    }

    /// 列出所有已注册的 Provider 信息。
    ///
    /// # Returns
    ///
    /// 返回所有 Provider 的 `ProviderInfo` 列表（含名称、显示名、scope、图标、品牌色）。
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
