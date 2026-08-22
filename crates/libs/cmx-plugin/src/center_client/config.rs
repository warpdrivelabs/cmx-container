//! 服务中心客户端配置。
//!
//! 从 `dev.toml` 的 `[center_client]` 配置节加载服务定位配置。配置优先级：
//! dev.toml < 环境变量（`CENTER_CLIENT__*`，如 `CENTER_CLIENT__URLS__FLOW`）。
//!
//! 「服务定位」为**自由键值表**（map）：`[center_client.urls]`（手动基址）与
//! `[center_client.discovery.services]`（Nacos 服务名）都不是固定字段——新增微服务只需
//! 在 toml 加一行键值，本配置层零代码改动。键所有权约定：
//!   - `menu` / `perm` / `form`：归远程导入器（值 = 门户/能力中心基址）；
//!   - `flow` / `report` / `rules`：归反向代理（值 = 独立微服务基址，见 [`crate::center_client::upstream`]）。
//!
//! 值语义：两类表的值均为**纯基址 / 服务名**，不含路径——导入端点与反代路径由消费方拼接。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// 服务中心客户端配置。
///
/// 对应 `dev.toml` 中 `[center_client]` 配置节。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CenterClientConfig {
    /// 访问模式：`"local"`（默认）| `"grpc"` | `"http_url"` | `"http_discovery"`。
    ///
    /// mode 同时决定反代目标来源（http_url → urls 表；http_discovery/grpc → discovery.services 表；
    /// local / 未知值 → 不挂反代），见 [`crate::center_client::upstream::proxy_upstream`]。
    #[serde(default = "default_mode")]
    pub mode: String,

    /// URL 直连模式（mode = "http_url"）：服务键 → 手动基址（自由表，值为纯基址无路径）。
    #[serde(default)]
    pub urls: HashMap<String, String>,

    /// 服务发现模式配置。
    #[serde(default)]
    pub discovery: CenterDiscoveryConfig,

    /// 请求超时时间（毫秒）。
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
}

/// 服务发现模式配置。
///
/// 对应 `dev.toml` 中 `[center_client.discovery]` 配置节。`services` 为自由键值表
/// （服务键 → Nacos 服务名），新增微服务加一行即可。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CenterDiscoveryConfig {
    /// Nacos 分组。
    #[serde(default)]
    pub nacos_group: Option<String>,
    /// 服务键 → Nacos 服务名（mode = "http_discovery" / "grpc" 时生效）。
    #[serde(default)]
    pub services: HashMap<String, String>,
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
    /// 2. 环境变量 `CENTER_CLIENT__MODE`、`CENTER_CLIENT__URLS__<KEY>`、
    ///    `CENTER_CLIENT__DISCOVERY__SERVICES__<KEY>` 等。
    ///
    /// # Returns
    ///
    /// 反序列化成功的配置；`ConfigManager` 未初始化、配置节缺失或解析失败时返回默认配置
    /// （mode = local，全内嵌、不挂反代），并以日志区分三种回退情形。
    pub fn load() -> Self {
        let config_manager = match cmx_utils::config::ConfigManager::try_global() {
            Some(cm) => cm,
            None => {
                tracing::warn!("ConfigManager 未初始化，使用默认 center_client 配置 (mode=local)");
                return Self::default();
            }
        };

        let sub = match config_manager.sub_config("center_client") {
            Ok(s) => s,
            Err(_) => {
                tracing::info!("未找到 center_client 配置节，使用默认 mode=local（全内嵌、不挂反代）");
                return Self::default();
            }
        };

        match sub.deserialize::<Self>() {
            Ok(config) => {
                warn_legacy_endpoint_values(&config);
                config
            }
            Err(e) => {
                tracing::warn!("加载 center_client 配置失败: {e}，使用默认 mode=local");
                Self::default()
            }
        }
    }
}

/// 对 urls 表中残留的旧「完整端点」写法打 warn（值应为纯基址，含路径会拼出双路径）。
fn warn_legacy_endpoint_values(config: &CenterClientConfig) {
    for (key, val) in config.urls.iter() {
        if val.contains("/api/") {
            tracing::warn!(
                url.key = %key,
                url.value = %val,
                "center_client.urls 的值应为纯基址（不含路径），检测到旧「完整端点」写法，将被追加统一导入端点路径"
            );
        }
    }
}

impl Default for CenterClientConfig {
    fn default() -> Self {
        Self {
            mode: default_mode(),
            urls: HashMap::new(),
            discovery: CenterDiscoveryConfig::default(),
            timeout_ms: default_timeout(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// urls / discovery.services 两张自由表可从嵌套 toml 结构反序列化，未知键不报错。
    #[test]
    fn deserialize_maps_from_nested_tables() {
        let toml_text = r#"
            mode = "http_url"
            [urls]
            flow = "http://127.0.0.1:8091"
            some_future_service = "http://10.0.0.5:9000"
            [discovery]
            nacos_group = "DEFAULT_GROUP"
            [discovery.services]
            menu = "cmx-portal-local"
            flow = "cmx-flow-server"
        "#;
        let cfg: CenterClientConfig = toml::from_str(toml_text).expect("应反序列化成功");
        assert_eq!(cfg.mode, "http_url");
        assert_eq!(cfg.urls.get("flow").map(String::as_str), Some("http://127.0.0.1:8091"));
        // 未知服务键（未来新增微服务）原样进入 map，不被结构体拒收。
        assert!(cfg.urls.contains_key("some_future_service"));
        assert_eq!(
            cfg.discovery.services.get("menu").map(String::as_str),
            Some("cmx-portal-local")
        );
        assert_eq!(cfg.discovery.nacos_group.as_deref(), Some("DEFAULT_GROUP"));
    }

    /// 缺省字段回退默认值（mode=local、空表）。
    #[test]
    fn defaults_are_local_and_empty() {
        let cfg: CenterClientConfig = toml::from_str("").expect("空配置应反序列化成功");
        assert_eq!(cfg.mode, "local");
        assert!(cfg.urls.is_empty());
        assert!(cfg.discovery.services.is_empty());
        assert_eq!(cfg.timeout_ms, 30000);
    }
}
