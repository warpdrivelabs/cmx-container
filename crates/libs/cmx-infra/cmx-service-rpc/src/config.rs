//! `[service_rpc]` 配置（服务间统一调用目录）。
//!
//! 从生效 toml 的 `[service_rpc]` 节加载，取代旧 `[center_client]`（服务定位）与
//! `[rpc]`（gRPC 服务端设施，并入 `[service_rpc.server]`；`[service_auth]` 语义独立、
//! 保留原段名）。配置优先级：toml < 远程配置中心 < 环境变量（`SERVICE_RPC__*`，如
//! `SERVICE_RPC__SERVICES__FLOW__URL`，值带 scheme）。
//!
//! 目录是 **per-key 自由表**（`[service_rpc.services]`，服务键 → 服务描述），新增微服务
//! 只在 toml 加一行，本层零代码改动。两个正交维度按键独立：
//! - **定位**（怎么找到目标）：`url`（静态基址，★无注册中心的回滚形态）或 `discovery`
//!   （Nacos 服务名）。并存时 **url 优先**（灰度切换语义：删 url 即切发现选例）；
//! - **传输**（怎么通信）：`transport` = `"http"` | `"grpc"`，缺省取全局 `default_transport`。
//!
//! ```toml
//! [service_rpc]
//! default_transport = "http"
//! timeout_ms = 30000
//! retry_max = 1
//!
//! [service_rpc.services]
//! flow  = { url = "http://127.0.0.1:8091" }
//! model = { discovery = "cmx-model-server" }
//!
//! [service_rpc.server]          # gRPC 服务端设施（grpc-server feature 下生效）
//! enabled = false
//! grpc_port = 0
//! warmup_services = []
//! ```
//!
//! 加载失败语义（fail-fast 判定矩阵，见 [`try_load`]）：
//! - 段缺失 → **合法空目录**（全内嵌 / 零出站形态，rules 等服务不需要该段）；
//! - 解析失败 / 旧段残留（`[center_client]` / `[rpc]`）→ **Err**（启动显性报错，不做兼容读取）。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::error::ServiceRpcError;

/// 单个服务键的目录描述（`[service_rpc.services].{key}` 的值）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServiceEntry {
    /// 静态基址（定位方式一，与 `discovery` 二选一）。值为纯基址（不含路径）；
    /// 与 `discovery` 并存时 url 优先（非主备兜底：url 不可达即 502 不会切发现）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Nacos 服务名（定位方式二）。`transport = "grpc"` 时必配（gRPC 按服务名路由）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discovery: Option<String>,
    /// 传输协议覆盖："http" | "grpc"。缺省取全局 `default_transport`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transport: Option<String>,
    /// 键级请求总超时（毫秒），覆盖全局 `timeout_ms`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    /// 键级幂等重试次数上限，覆盖全局 `retry_max`（仅幂等调用 + 传输级连接错误换实例重试）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_max: Option<u32>,
}

/// gRPC 服务端与客户端设施参数（原 `[rpc.grpc]` 段，grpc feature 下消费）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrpcConfig {
    /// 本进程 gRPC 监听端口。
    #[serde(default)]
    pub port: u16,
    /// 调用超时（毫秒）。
    #[serde(default = "default_grpc_timeout")]
    pub timeout_ms: u64,
    /// 连接超时（毫秒）。
    #[serde(default = "default_grpc_connect_timeout")]
    pub connect_timeout_ms: u64,
    /// 客户端重试次数（0 = 不重试）。
    #[serde(default)]
    pub retry_count: usize,
    /// 服务发现缺省分组。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_group: Option<String>,
    /// 服务发现缺省集群列表。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub default_clusters: Vec<String>,
    /// 发现通道容量。
    #[serde(default = "default_discover_channel_capacity")]
    pub discover_channel_capacity: usize,
}

impl Default for GrpcConfig {
    fn default() -> Self {
        Self {
            port: 0,
            timeout_ms: default_grpc_timeout(),
            connect_timeout_ms: default_grpc_connect_timeout(),
            retry_count: 0,
            default_group: None,
            default_clusters: Vec::new(),
            discover_channel_capacity: default_discover_channel_capacity(),
        }
    }
}

/// 服务端设施开关与参数（原 `[rpc]` 段并入 `[service_rpc.server]`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    /// 是否启用 gRPC 服务端设施。
    pub enabled: bool,
    /// 协议（当前仅 "grpc"）。
    pub protocol: String,
    /// gRPC 参数。
    pub grpc: GrpcConfig,
    /// 启动预订阅的服务名清单（填充实例缓存，消除首调冷启动）。
    pub warmup_services: Vec<String>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            protocol: "grpc".to_string(),
            grpc: GrpcConfig::default(),
            warmup_services: Vec::new(),
        }
    }
}

/// 服务间统一调用目录配置（`[service_rpc]` 节）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceRpcConfig {
    /// 服务间调用的全局传输缺省：`"http"`（默认）| `"grpc"`。键级 `transport` 可覆盖。
    #[serde(default = "default_transport")]
    pub default_transport: String,

    /// Nacos 分组（所有 `discovery` 定位的服务键共用，缺省 DEFAULT_GROUP）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nacos_group: Option<String>,

    /// 请求总超时（毫秒），键级 `timeout_ms` 可覆盖。
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,

    /// 幂等调用重试次数上限（仅幂等方法 + 传输级连接错误，换实例重试），键级可覆盖。
    #[serde(default = "default_retry_max")]
    pub retry_max: u32,

    /// 服务键 → 服务描述（定位 + 传输 + 可选超时/重试覆盖），自由表。
    #[serde(default)]
    pub services: HashMap<String, ServiceEntry>,

    /// gRPC 服务端设施（原 `[rpc]` 段）。
    #[serde(default)]
    pub server: ServerConfig,
}

fn default_transport() -> String {
    "http".to_string()
}

fn default_timeout() -> u64 {
    30_000
}

fn default_retry_max() -> u32 {
    1
}

fn default_grpc_timeout() -> u64 {
    30_000
}

fn default_grpc_connect_timeout() -> u64 {
    3_000
}

fn default_discover_channel_capacity() -> usize {
    1024
}

impl Default for ServiceRpcConfig {
    fn default() -> Self {
        Self {
            default_transport: default_transport(),
            nacos_group: None,
            timeout_ms: default_timeout(),
            retry_max: default_retry_max(),
            services: HashMap::new(),
            server: ServerConfig::default(),
        }
    }
}

impl ServiceRpcConfig {
    /// 从全局 `ConfigManager` 加载并做 fail-fast 校验（初始化入口用）。
    ///
    /// # Fail-fast 判定矩阵
    ///
    /// - 段缺失 → `Ok(默认空目录)`（合法：全内嵌 / 零出站服务不需要该段）；
    /// - 段存在但反序列化失败 → `Err(Decode)`；
    /// - 旧段残留（`[center_client]` / `[rpc]`）→ `Err(Decode)`，错误信息带迁移提示
    ///   （不做兼容读取，错配显性化）。
    pub fn try_load() -> Result<Self, ServiceRpcError> {
        let key = "service_rpc";
        let Some(cm) = cmx_utils::config::ConfigManager::try_global() else {
            // ConfigManager 未初始化（单测 / 非服务进程）：合法空目录。
            return Ok(Self::default());
        };
        // 旧段残留检测：改名迁移显性报错，防静默丢配置。
        if cm.sub_config("center_client").is_ok() {
            return Err(ServiceRpcError::Decode {
                key: key.to_string(),
                cause: "检测到旧配置段 [center_client]：已更名为 [service_rpc]"
                    .to_string(),
            });
        }
        if cm.sub_config("rpc").is_ok() {
            return Err(ServiceRpcError::Decode {
                key: key.to_string(),
                cause: "检测到旧配置段 [rpc]：已并入 [service_rpc.server]".to_string(),
            });
        }
        let sub = match cm.sub_config(key) {
            Ok(s) => s,
            Err(_) => {
                tracing::info!("未找到 [service_rpc] 配置节，使用空目录（全内嵌 / 零出站形态）");
                return Ok(Self::default());
            }
        };
        match sub.deserialize::<ServiceRpcConfig>() {
            Ok(config) => {
                warn_misplaced_values(&config);
                Ok(config)
            }
            Err(e) => Err(ServiceRpcError::Decode {
                key: key.to_string(),
                cause: format!("[service_rpc] 段解析失败: {e}"),
            }),
        }
    }

    /// 便捷加载（不报错）：任何失败回退默认空目录并打日志。非初始化路径的兜底读取用，
    /// 服务启动初始化必须走 [`Self::try_load`]。
    pub fn load() -> Self {
        Self::try_load().unwrap_or_else(|e| {
            tracing::warn!(error = %e, "service_rpc 配置加载失败，回退空目录");
            Self::default()
        })
    }

    /// 解析指定服务键的生效传输。
    ///
    /// 键级 `transport` 覆盖全局 `default_transport`（空白视同未配置）；
    /// 两处均为空白或未知值时回退 `"http"`（warn 提示，单键笔误不拖垮整表）。
    pub fn transport_of(&self, key: &str) -> TransportKind {
        let key_transport = self
            .services
            .get(key)
            .and_then(|e| e.transport.as_deref())
            .map(str::trim)
            .filter(|s| !s.is_empty());
        match key_transport.unwrap_or(self.default_transport.trim()) {
            "grpc" => TransportKind::Grpc,
            _ => TransportKind::Http,
        }
    }
}

/// 生效传输类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportKind {
    /// HTTP/REST（默认传输，基座默认 feature 即备）。
    Http,
    /// gRPC（需 `grpc-client` feature + 契约 SDK 的 gRPC 绑定）。
    Grpc,
}

impl TransportKind {
    /// 人读名。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Grpc => "grpc",
        }
    }
}

/// 对易错的值写法打 warn：url 含路径、grpc 传输配了 url、transport 值域外。
fn warn_misplaced_values(config: &ServiceRpcConfig) {
    for (key, entry) in config.services.iter() {
        if let Some(url) = &entry.url
            && url.contains("/api/")
        {
            tracing::warn!(
                service.key = %key,
                url.value = %url,
                "services.{key}.url 应为纯基址（不含路径），消费方会自行拼接端点路径"
            );
        }
        if entry.transport.as_deref().map(str::trim) == Some("grpc")
            && entry.url.as_deref().map(str::trim).filter(|s| !s.is_empty()).is_some()
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
            "service_rpc.default_transport 值域应为 http|grpc，未知值已回退 http"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// services 单表反序列化：url/discovery/transport/timeout_ms/retry_max 各形态、
    /// 未知键不拒收、server 子段并入解析。
    #[test]
    fn deserialize_services_table() {
        let toml_text = r#"
            default_transport = "http"
            timeout_ms = 20000
            retry_max = 2
            [services]
            flow = { url = "http://127.0.0.1:8091", timeout_ms = 10000 }
            model = { discovery = "cmx-model-server" }
            perm = { discovery = "cmx-portal-local", transport = "grpc" }
            future_svc = { url = "http://10.0.0.5:9000" }
            [server]
            enabled = false
            [server.grpc]
            port = 9090
        "#;
        let cfg: ServiceRpcConfig = toml::from_str(toml_text).expect("应反序列化成功");
        assert_eq!(cfg.timeout_ms, 20000);
        assert_eq!(cfg.retry_max, 2);
        let flow = cfg.services.get("flow").expect("flow 键应存在");
        assert_eq!(flow.url.as_deref(), Some("http://127.0.0.1:8091"));
        assert_eq!(flow.timeout_ms, Some(10000));
        assert_eq!(
            cfg.services.get("perm").and_then(|e| e.transport.as_deref()),
            Some("grpc")
        );
        assert!(cfg.services.contains_key("future_svc"));
        assert_eq!(cfg.server.grpc.port, 9090);
        assert!(!cfg.server.enabled);
    }

    /// 缺省字段回退（http / 空表 / 30s 超时 / 重试 1 次 / server 关闭）。
    #[test]
    fn defaults_are_http_and_empty() {
        let cfg: ServiceRpcConfig = toml::from_str("").expect("空配置应反序列化成功");
        assert_eq!(cfg.default_transport, "http");
        assert!(cfg.services.is_empty());
        assert_eq!(cfg.timeout_ms, 30000);
        assert_eq!(cfg.retry_max, 1);
        assert!(!cfg.server.enabled);
        assert_eq!(cfg.server.grpc.timeout_ms, 30000);
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
        let cfg: ServiceRpcConfig = toml::from_str(toml_text).expect("应反序列化成功");
        assert_eq!(cfg.transport_of("menu"), TransportKind::Http);
        assert_eq!(cfg.transport_of("perm"), TransportKind::Grpc);
        assert_eq!(cfg.transport_of("form"), TransportKind::Grpc);
        assert_eq!(cfg.transport_of("unknown"), TransportKind::Grpc);
    }
}
