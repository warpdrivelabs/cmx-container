//! 服务中心客户端配置（服务定位 + 传输协议，均按服务键独立配置）。
//!
//! 从 `dev.toml` 的 `[center_client]` 配置节加载。配置优先级：
//! dev.toml < 环境变量（`CENTER_CLIENT__*`，如 `CENTER_CLIENT__SERVICES__FLOW__URL`）。
//!
//! 两个正交维度均为 **per-key 自由表**（`[center_client.services]`，键 → 服务描述）：
//! - **定位**（怎么找到目标）：`url`（静态基址）或 `discovery`（Nacos 服务名），二选一——
//!   不同服务键可混用（如 flow 本机静态调试、report 走 Nacos）；
//! - **传输**（怎么通信，仅服务间调用生效）：`transport` = `"http"` | `"grpc"`，缺省取全局
//!   `default_transport`——不同中心可混用（如 menu 中心 gRPC、form 中心 HTTP）。
//!   反向代理恒走 HTTP（透明转发语义），`transport` 对反代键无效（配 grpc 会打 warn）。
//!
//! 新增微服务只需在 toml 加一行键值，本配置层零代码改动。键所有权约定：
//!   - `menu` / `perm` / `form`：归远程导入器（目标 = 门户/能力中心）；
//!   - `flow` / `report` / `rules`：归反向代理（目标 = 独立微服务，见 [`crate::center_client::upstream`]）。
//!
//! 值语义：`url` 为**纯基址**（不含路径）——导入端点与反代路径由消费方拼接。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// 单个服务键的定位与传输描述（`[center_client.services].{key}` 的值）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServiceEntry {
    /// 静态基址（定位方式一，与 `discovery` 二选一）。值为纯基址，不含路径。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Nacos 服务名（定位方式二，与 `url` 二选一）。`transport = "grpc"` 时必配
    /// （gRPC 经全局 RPC 客户端按服务名路由，不支持静态地址直连）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discovery: Option<String>,
    /// 传输协议（仅服务间调用生效）："http" | "grpc"。缺省取全局 `default_transport`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transport: Option<String>,
}

/// 服务中心客户端配置。
///
/// 对应 toml 中 `[center_client]` 配置节。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CenterClientConfig {
    /// 服务间调用的全局传输缺省：`"http"`（默认）| `"grpc"`。键级 `transport` 可覆盖。
    #[serde(default = "default_transport")]
    pub default_transport: String,

    /// Nacos 分组（所有 `discovery` 定位的服务键共用，缺省 DEFAULT_GROUP）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nacos_group: Option<String>,

    /// 服务键 → 服务描述（定位 + 可选传输覆盖），自由表。
    #[serde(default)]
    pub services: HashMap<String, ServiceEntry>,

    /// 请求超时时间（毫秒）。
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
}

fn default_transport() -> String {
    "http".to_string()
}

fn default_timeout() -> u64 {
    30000
}

impl CenterClientConfig {
    /// 从全局 `ConfigManager` 加载配置。
    ///
    /// 配置优先级（从低到高）：
    /// 1. toml 的 `[center_client]` 节。
    /// 2. 环境变量 `CENTER_CLIENT__DEFAULT_TRANSPORT`、`CENTER_CLIENT__SERVICES__<KEY>__URL` 等。
    ///
    /// # Returns
    ///
    /// 反序列化成功的配置；`ConfigManager` 未初始化、配置节缺失或解析失败时返回默认配置
    /// （空 services 表 = 全内嵌、不挂反代），并以日志区分三种回退情形。
    pub fn load() -> Self {
        let config_manager = match cmx_utils::config::ConfigManager::try_global() {
            Some(cm) => cm,
            None => {
                tracing::warn!("ConfigManager 未初始化，使用默认 center_client 配置 (空 services)");
                return Self::default();
            }
        };

        let sub = match config_manager.sub_config("center_client") {
            Ok(s) => s,
            Err(_) => {
                tracing::info!("未找到 center_client 配置节，使用默认（空 services，全内嵌、不挂反代）");
                return Self::default();
            }
        };

        // 先中转 toml::Value：检测旧配置形态（mode/urls/discovery，已被忽略），避免静默丢配置。
        match sub.deserialize::<toml::Value>() {
            Ok(raw) => {
                warn_legacy_shape(&raw);
                match CenterClientConfig::deserialize(raw) {
                    Ok(config) => {
                        warn_misplaced_values(&config);
                        config
                    }
                    Err(e) => {
                        tracing::warn!("加载 center_client 配置失败: {e}，使用默认（空 services）");
                        Self::default()
                    }
                }
            }
            Err(e) => {
                tracing::warn!("加载 center_client 配置失败: {e}，使用默认（空 services）");
                Self::default()
            }
        }
    }

    /// 解析指定服务键的生效传输协议（服务间调用用）。
    ///
    /// 键级 `transport` 覆盖全局 `default_transport`（空白视同未配置）；两处均为空白或
    /// 未知值时回退 `"http"`。
    ///
    /// # Arguments
    ///
    /// * `key` - 服务键（如 "menu" / "perm" / "form"）。
    pub fn transport_of(&self, key: &str) -> &'static str {
        let key_transport = self
            .services
            .get(key)
            .and_then(|e| e.transport.as_deref())
            .map(str::trim)
            .filter(|s| !s.is_empty());
        match key_transport.unwrap_or(self.default_transport.trim()) {
            "grpc" => "grpc",
            _ => "http",
        }
    }

    /// 服务表中配置了任一远程导入键（menu/perm/form）。
    ///
    /// 供装配层（cmx-platform-app iam.rs）决定 `DefinitionImporterBundle` 取本地还是远程实现。
    pub fn has_remote_import_keys(&self) -> bool {
        ["menu", "perm", "form"]
            .iter()
            .any(|k| self.services.contains_key(*k))
    }
}

/// 对旧版配置形态（mode / urls / discovery 三段，v2 已废弃）打 warn，避免静默丢配置。
fn warn_legacy_shape(raw: &toml::Value) {
    let legacy_keys: Vec<&str> = ["mode", "urls", "discovery"]
        .into_iter()
        .filter(|k| raw.get(*k).is_some())
        .collect();
    if !legacy_keys.is_empty() {
        tracing::warn!(
            legacy_keys = ?legacy_keys,
            "检测到旧版 center_client 配置形态（mode/urls/discovery），已被忽略——\
             请迁移到 [center_client.services] 单表（per-key 定位+传输），见 config_template.toml"
        );
    }
}

/// 对易错的值写法打 warn：url 含路径（会拼出双路径）、grpc 传输配了 url（无效）、
/// transport 为未知值域（回退 http）。
fn warn_misplaced_values(config: &CenterClientConfig) {
    for (key, entry) in config.services.iter() {
        if let Some(url) = &entry.url
            && url.contains("/api/")
        {
            tracing::warn!(
                service.key = %key,
                url.value = %url,
                "services.{key}.url 应为纯基址（不含路径），检测到旧「完整端点」写法，将被追加统一导入端点路径"
            );
        }
        if entry.transport.as_deref().map(str::trim) == Some("grpc")
            && entry.url.is_some()
        {
            tracing::warn!(
                service.key = %key,
                "services.{key} 配置 transport=grpc 时 url 无效（gRPC 经服务名路由），请改配 discovery"
            );
        }
        if let Some(t) = entry.transport.as_deref().map(str::trim).filter(|s| !s.is_empty())
            && t != "http"
            && t != "grpc"
        {
            tracing::warn!(
                service.key = %key,
                transport = %t,
                "services.{key}.transport 值域应为 http|grpc，未知值已回退 http"
            );
        }
    }
    let dt = config.default_transport.trim();
    if !dt.is_empty() && dt != "http" && dt != "grpc" {
        tracing::warn!(
            default_transport = %config.default_transport,
            "center_client.default_transport 值域应为 http|grpc，未知值已回退 http"
        );
    }
}

impl Default for CenterClientConfig {
    fn default() -> Self {
        Self {
            default_transport: default_transport(),
            nacos_group: None,
            services: HashMap::new(),
            timeout_ms: default_timeout(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// services 单表可从嵌套 toml 结构反序列化：url/discovery/transport 各形态、未知键不报错。
    #[test]
    fn deserialize_services_table() {
        let toml_text = r#"
            default_transport = "grpc"
            nacos_group = "DEFAULT_GROUP"
            [services]
            flow = { url = "http://127.0.0.1:8091" }
            report = { discovery = "cmx-rpt-server" }
            menu = { discovery = "cmx-portal-local", transport = "grpc" }
            some_future_service = { url = "http://10.0.0.5:9000" }
        "#;
        let cfg: CenterClientConfig = toml::from_str(toml_text).expect("应反序列化成功");
        assert_eq!(cfg.default_transport, "grpc");
        assert_eq!(cfg.nacos_group.as_deref(), Some("DEFAULT_GROUP"));
        assert_eq!(
            cfg.services.get("flow").and_then(|e| e.url.as_deref()),
            Some("http://127.0.0.1:8091")
        );
        assert_eq!(
            cfg.services.get("report").and_then(|e| e.discovery.as_deref()),
            Some("cmx-rpt-server")
        );
        assert_eq!(
            cfg.services.get("menu").and_then(|e| e.transport.as_deref()),
            Some("grpc")
        );
        // 未知服务键（未来新增微服务）原样进入 map，不被结构体拒收。
        assert!(cfg.services.contains_key("some_future_service"));
    }

    /// 缺省字段回退默认值（default_transport=http、空表）。
    #[test]
    fn defaults_are_http_and_empty() {
        let cfg: CenterClientConfig = toml::from_str("").expect("空配置应反序列化成功");
        assert_eq!(cfg.default_transport, "http");
        assert!(cfg.nacos_group.is_none());
        assert!(cfg.services.is_empty());
        assert_eq!(cfg.timeout_ms, 30000);
    }

    /// 旧版字段（mode/urls/discovery）出现在 toml 里不报错且被忽略（load 时另有迁移 warn）。
    #[test]
    fn legacy_fields_are_ignored() {
        let toml_text = r#"
            mode = "http_url"
            [urls]
            flow = "http://127.0.0.1:8091"
            [discovery]
            nacos_group = "DEFAULT_GROUP"
            [discovery.services]
            menu = "cmx-portal-local"
            [services]
            flow = { url = "http://127.0.0.1:8092" }
        "#;
        let cfg: CenterClientConfig = toml::from_str(toml_text).expect("应反序列化成功");
        // services 表生效；旧 urls 值不混入。
        assert_eq!(
            cfg.services.get("flow").and_then(|e| e.url.as_deref()),
            Some("http://127.0.0.1:8092")
        );
        assert!(cfg.services.get("menu").is_none());
    }

    /// transport_of 覆盖链：键级覆盖全局；空白/未知值回退 http。
    #[test]
    fn transport_of_precedence() {
        let toml_text = r#"
            default_transport = "grpc"
            [services]
            menu = { discovery = "cmx-portal-local", transport = "http" }
            perm = { discovery = "cmx-portal-local" }
            form = { discovery = "cmx-portal-local", transport = "  " }
        "#;
        let cfg: CenterClientConfig = toml::from_str(toml_text).expect("应反序列化成功");
        // 键级显式 http 覆盖全局 grpc。
        assert_eq!(cfg.transport_of("menu"), "http");
        // 未配键级 → 全局 grpc。
        assert_eq!(cfg.transport_of("perm"), "grpc");
        // 空白值视同未配置 → 全局 grpc。
        assert_eq!(cfg.transport_of("form"), "grpc");
        // 键不存在 → 全局。
        assert_eq!(cfg.transport_of("unknown"), "grpc");
    }

    /// has_remote_import_keys：menu/perm/form 任一存在即远程模式。
    #[test]
    fn remote_import_keys_detection() {
        let toml_text = r#"
            [services]
            menu = { discovery = "cmx-portal-local" }
        "#;
        let cfg: CenterClientConfig = toml::from_str(toml_text).expect("应反序列化成功");
        assert!(cfg.has_remote_import_keys());

        let cfg: CenterClientConfig = toml::from_str(r#" [services]
            flow = { url = "http://127.0.0.1:8091" } "#)
            .expect("应反序列化成功");
        // 只有反代键（flow）→ 仍全本地导入。
        assert!(!cfg.has_remote_import_keys());
    }
}
