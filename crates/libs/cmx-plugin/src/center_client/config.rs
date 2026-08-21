//! 服务中心客户端配置。
//!
//! 从 `dev.toml` 的 `[center_client]` 配置节加载服务中心访问配置。
//! 支持四种模式：`mock`（默认）、`http_url`（URL 直连）、`http_discovery`（服务发现）、`grpc`（gRPC 调用）。
//! 配置优先级：dev.toml < 环境变量（`CENTER_CLIENT__*`）。

use super::types::DataCategory;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 服务中心客户端配置。
///
/// 对应 `dev.toml` 中 `[center_client]` 配置节。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CenterClientConfig {
    /// 访问模式：`"local"`（默认）| `"grpc"` | `"http_url"` | `"http_discovery"`。
    #[serde(default = "default_mode")]
    pub mode: String,

    /// URL 直连模式配置。
    #[serde(default)]
    pub urls: CenterUrlsConfig,

    /// 服务发现模式配置。
    #[serde(default)]
    pub discovery: CenterDiscoveryConfig,

    /// 请求超时时间（毫秒）。
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
}

/// URL 直连模式配置。
///
/// 每个中心对应一个独立的 URL 配置项。
/// 环境变量覆盖示例：`CENTER_CLIENT__URLS__MENU=http://...`。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CenterUrlsConfig {
    /// 门户中心（菜单数据）URL。
    #[serde(default)]
    pub menu: Option<String>,
    /// 权限中心（权限数据）URL。
    #[serde(default)]
    pub perm: Option<String>,
    /// 表单中心（表单数据）URL。
    #[serde(default)]
    pub form: Option<String>,
    /// 流程中心（流程定义）URL。
    #[serde(default)]
    pub flow: Option<String>,
    /// 报表中心（独立报表微服务 cmx-rpt-server）URL。非空=平台反代到它，空=进程内嵌。
    #[serde(default)]
    pub report: Option<String>,
    /// 决策规则中心（独立规则微服务 cmx-rule-server）URL。非空=平台反代到它；规则无内嵌，空=门户无规则路由。
    #[serde(default)]
    pub rules: Option<String>,
}

/// 服务发现模式配置。
///
/// 通过 Nacos 服务发现获取各中心的实例地址。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CenterDiscoveryConfig {
    /// Nacos 分组。
    #[serde(default)]
    pub nacos_group: Option<String>,
    /// 门户中心服务名。
    #[serde(default)]
    pub menu_service: Option<String>,
    /// 权限中心服务名。
    #[serde(default)]
    pub perm_service: Option<String>,
    /// 表单中心服务名。
    #[serde(default)]
    pub form_service: Option<String>,
    /// 流程中心服务名。
    #[serde(default)]
    pub flow_service: Option<String>,
}

impl CenterDiscoveryConfig {
    /// 获取指定数据类别对应的服务名。
    pub fn get_service_name(&self, category: DataCategory) -> Option<&str> {
        match category {
            DataCategory::Menu => self.menu_service.as_deref(),
            DataCategory::Perm => self.perm_service.as_deref(),
            DataCategory::Form => self.form_service.as_deref(),
            DataCategory::Flow => self.flow_service.as_deref(),
        }
    }
}

fn default_mode() -> String {
    "local".to_string()
}

fn default_timeout() -> u64 {
    30000
}

impl CenterClientConfig {
    /// 从全局 `ConfigManager` 加载配置。
    ///
    /// 配置优先级（从低到高）：
    /// 1. `dev.toml` 的 `[center_client]` 节。
    /// 2. 环境变量 `CENTER_CLIENT__MODE`、`CENTER_CLIENT__URLS__MENU` 等。
    ///
    /// 当 `ConfigManager` 未初始化或配置节不存在时，返回默认配置（mock 模式）。
    pub fn load() -> Self {
        let config_manager = match cmx_utils::config::ConfigManager::try_global() {
            Some(cm) => cm,
            None => {
                tracing::warn!("ConfigManager 未初始化，使用默认 center_client 配置 (mock)");
                return Self::default();
            }
        };

        let sub = match config_manager.sub_config("center_client") {
            Ok(s) => s,
            Err(_) => {
                tracing::info!("未找到 center_client 配置节，使用默认 mock 模式");
                return Self::default();
            }
        };

        match sub.deserialize::<Self>() {
            Ok(config) => {
                // tracing::info!("center_client 配置加载成功: mode={}", config.mode);
                config
            }
            Err(e) => {
                tracing::warn!("加载 center_client 配置失败: {}，使用默认 mock 模式", e);
                Self::default()
            }
        }
    }

    /// 解析 URL 配置为 `HashMap<DataCategory, String>`。
    ///
    /// 仅返回已配置的中心的 URL 映射。
    pub fn resolve_urls(&self) -> HashMap<DataCategory, String> {
        let mut urls = HashMap::new();
        if let Some(ref url) = self.urls.menu {
            urls.insert(DataCategory::Menu, url.clone());
        }
        if let Some(ref url) = self.urls.perm {
            urls.insert(DataCategory::Perm, url.clone());
        }
        if let Some(ref url) = self.urls.form {
            urls.insert(DataCategory::Form, url.clone());
        }
        if let Some(ref url) = self.urls.flow {
            urls.insert(DataCategory::Flow, url.clone());
        }
        urls
    }
}

impl Default for CenterClientConfig {
    fn default() -> Self {
        Self {
            mode: default_mode(),
            urls: CenterUrlsConfig::default(),
            discovery: CenterDiscoveryConfig::default(),
            timeout_ms: default_timeout(),
        }
    }
}
