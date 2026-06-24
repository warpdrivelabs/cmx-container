//! 配置模型定义。
//!
//! 该模块集中定义注册中心和配置中心的通用配置结构，
//! 支持从环境变量加载，并保持对历史 `NACOS_*` 环境变量的完全兼容。
//!
//! # 主要类型
//!
//! - [`RegistryConfig`]：注册中心配置（类型、启用标志、通用服务参数 + Nacos 子配置）。
//! - [`NacosNamingConfig`]：Nacos 命名服务的连接配置。
//! - [`ConfigCenterFullConfig`]：配置中心配置（类型、启用标志、监听列表 + Nacos 子配置）。
//! - [`NacosConfigCenterConfig`]：Nacos 配置中心的连接配置。
//! - [`ConfigListener`]：配置监听项，描述 `data_id/group`。
//!
//! # 环境变量约定
//!
//! 所有配置都可通过 `from_env()` 静态方法从进程环境变量构建，
//! 优先读取 `SERVICE_REGISTRY_*` / `CONFIG_CENTER_*` 新前缀，回退到 `NACOS_*` 旧前缀。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// ============================================================
// 注册中心配置
// ============================================================

/// 注册中心配置。
///
/// 包含通用的服务注册参数（与服务实例相关）和注册中心连接配置。
/// 通用参数对所有注册中心类型生效，具体注册中心的连接配置在子结构体中定义。
///
/// # Fields
///
/// * `registry_type` - 注册中心类型，识别 `mock` / `nacos` / `consul` / `etcd`。
/// * `enabled` - 是否启用服务注册。
/// * `service_name` - 注册的服务名称。
/// * `group_name` - 分组名称。
/// * `cluster_name` - 集群名称。
/// * `weight` - 实例权重，默认为 `1.0`。
/// * `metadata` - 实例元数据键值对。
/// * `nacos` - 当 `registry_type = "nacos"` 时生效的 Nacos 连接配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryConfig {
    /// 注册中心类型：`mock` | `nacos` | `consul` | `etcd`。
    #[serde(default = "default_registry_type")]
    pub registry_type: String,

    /// 是否启用服务注册。
    #[serde(default)]
    pub enabled: bool,

    /// 注册的服务名称。
    #[serde(default = "default_service_name")]
    pub service_name: String,

    /// 分组名称。
    #[serde(default = "default_group")]
    pub group_name: String,

    /// 集群名称。
    #[serde(default = "default_cluster")]
    pub cluster_name: String,

    /// 实例权重。
    #[serde(default = "default_weight")]
    pub weight: f64,

    /// 实例元数据。
    #[serde(default)]
    pub metadata: HashMap<String, String>,

    /// Nacos 注册中心配置，`type = "nacos"` 时生效。
    #[serde(default)]
    pub nacos: NacosNamingConfig,
}

/// Nacos 命名服务配置。
///
/// 仅包含 Nacos 连接相关的配置，不包含服务实例参数。
/// 服务实例参数（`service_name`、`group_name` 等）在 [`RegistryConfig`] 中定义。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NacosNamingConfig {
    /// Nacos 服务器地址（`host:port` 格式）。
    #[serde(default = "default_server_addr")]
    pub server_addr: String,

    /// 命名空间。
    #[serde(default)]
    pub namespace: String,

    /// 应用名称。
    #[serde(default = "default_app_name")]
    pub app_name: String,

    /// 认证用户名。
    #[serde(default)]
    pub username: Option<String>,

    /// 认证密码。
    #[serde(default)]
    pub password: Option<String>,
}

// ============================================================
// 配置中心配置
// ============================================================

/// 配置中心配置。
///
/// 描述配置中心的连接、启用状态以及要监听的 `data_id/group` 列表。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigCenterFullConfig {
    /// 配置中心类型：`mock` | `nacos` | `apollo`。
    #[serde(default = "default_center_type")]
    pub center_type: String,

    /// 是否启用配置中心。
    #[serde(default)]
    pub enabled: bool,

    /// Nacos 配置中心配置，`type = "nacos"` 时生效。
    #[serde(default)]
    pub nacos: NacosConfigCenterConfig,

    /// 配置监听列表。
    #[serde(default)]
    pub listeners: Vec<ConfigListener>,
}

/// Nacos 配置中心配置。
///
/// 仅包含 Nacos 配置中心连接相关的参数。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NacosConfigCenterConfig {
    /// Nacos 服务器地址（`host:port` 格式）。
    #[serde(default = "default_server_addr")]
    pub server_addr: String,

    /// 命名空间。
    #[serde(default)]
    pub namespace: String,

    /// 应用名称。
    #[serde(default = "default_app_name")]
    pub app_name: String,

    /// 认证用户名。
    #[serde(default)]
    pub username: Option<String>,

    /// 认证密码。
    #[serde(default)]
    pub password: Option<String>,
}

/// 配置监听项。
///
/// 描述一个 Nacos 配置项的 `data_id` 和 `group`，用于注册变更监听。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigListener {
    /// Data ID（配置标识）。
    pub data_id: String,

    /// Group（配置分组）。
    #[serde(default = "default_group")]
    pub group: String,
}

// ============================================================
// 默认值常量
// ============================================================

/// Nacos 服务器地址默认值。
pub const DEFAULT_NACOS_ADDR: &str = "127.0.0.1:8848";
/// 应用名称默认值。
pub const DEFAULT_APP_NAME: &str = "cmx-container";
/// 服务名称默认值。
pub const DEFAULT_SERVICE_NAME: &str = "cmx-server";
/// Nacos Group 默认值。
pub const DEFAULT_GROUP: &str = "DEFAULT_GROUP";
/// Nacos Cluster 默认值。
pub const DEFAULT_CLUSTER: &str = "DEFAULT";

// ============================================================
// 默认值函数
// ============================================================

/// 返回注册中心类型的默认值 `"mock"`。
fn default_registry_type() -> String {
    "mock".to_string()
}

/// 返回配置中心类型的默认值 `"mock"`。
fn default_center_type() -> String {
    "mock".to_string()
}

/// 返回 Nacos 服务器地址的默认值。
fn default_server_addr() -> String {
    DEFAULT_NACOS_ADDR.to_string()
}

/// 返回应用名称的默认值。
fn default_app_name() -> String {
    DEFAULT_APP_NAME.to_string()
}

/// 返回服务名称的默认值。
fn default_service_name() -> String {
    DEFAULT_SERVICE_NAME.to_string()
}

/// 返回 Nacos Group 的默认值。
fn default_group() -> String {
    DEFAULT_GROUP.to_string()
}

/// 返回 Nacos Cluster 的默认值。
fn default_cluster() -> String {
    DEFAULT_CLUSTER.to_string()
}

/// 返回实例权重的默认值 `1.0`。
fn default_weight() -> f64 {
    1.0
}

// ============================================================
// Default 实现
// ============================================================

impl Default for RegistryConfig {
    /// 返回所有字段均为默认值的 `RegistryConfig`。
    fn default() -> Self {
        Self {
            registry_type: default_registry_type(),
            enabled: false,
            service_name: default_service_name(),
            group_name: default_group(),
            cluster_name: default_cluster(),
            weight: default_weight(),
            metadata: HashMap::new(),
            nacos: NacosNamingConfig::default(),
        }
    }
}

impl Default for NacosNamingConfig {
    /// 返回所有字段均为默认值的 `NacosNamingConfig`。
    fn default() -> Self {
        Self {
            server_addr: default_server_addr(),
            namespace: String::new(),
            app_name: default_app_name(),
            username: None,
            password: None,
        }
    }
}

impl Default for ConfigCenterFullConfig {
    /// 返回所有字段均为默认值的 `ConfigCenterFullConfig`。
    fn default() -> Self {
        Self {
            center_type: default_center_type(),
            enabled: false,
            nacos: NacosConfigCenterConfig::default(),
            listeners: Vec::new(),
        }
    }
}

impl Default for NacosConfigCenterConfig {
    /// 返回所有字段均为默认值的 `NacosConfigCenterConfig`。
    fn default() -> Self {
        Self {
            server_addr: default_server_addr(),
            namespace: String::new(),
            app_name: default_app_name(),
            username: None,
            password: None,
        }
    }
}

// ============================================================
// 环境变量加载
// ============================================================

impl RegistryConfig {
    /// 从环境变量加载注册中心配置。
    ///
    /// # 读取的环境变量
    ///
    /// * `SERVICE_REGISTRY_TYPE` - 注册中心类型。
    /// * `SERVICE_REGISTRY_ENABLED` - 是否启用。
    /// * `SERVICE_REGISTRY_NAME` - 注册的服务名称（优先于 `NACOS_NAMING_SERVICE_NAME`）。
    /// * `SERVICE_REGISTRY_GROUP` - 分组名称。
    /// * `SERVICE_REGISTRY_CLUSTER` - 集群名称。
    /// * `SERVICE_REGISTRY_WEIGHT` - 实例权重。
    /// * 兼容 `NACOS_ENABLED` / `NACOS_NAMING_ENABLED` / `NACOS_NAMING_*` 旧前缀。
    ///
    /// # 兼容性规则
    ///
    /// 当 `NACOS_ENABLED=true` 且 `NACOS_NAMING_ENABLED=true`（默认 true）时，
    /// 自动选择 `nacos` 类型并启用注册中心。
    ///
    /// # Returns
    ///
    /// 返回从环境变量解析得到的 `RegistryConfig`。
    pub fn from_env() -> Self {
        let nacos_enabled = env_bool("NACOS_ENABLED");
        let naming_enabled = env_bool_or("NACOS_NAMING_ENABLED", true);

        // 兼容旧 NACOS_ENABLED：启用时自动设为 nacos 类型。
        let registry_type = if nacos_enabled && naming_enabled {
            env_string("SERVICE_REGISTRY_TYPE").unwrap_or_else(|| "nacos".to_string())
        } else {
            env_string("SERVICE_REGISTRY_TYPE").unwrap_or_else(default_registry_type)
        };

        let enabled = if nacos_enabled && naming_enabled {
            true
        } else {
            env_bool("SERVICE_REGISTRY_ENABLED")
        };

        Self {
            registry_type,
            enabled,
            service_name: env_string("SERVICE_REGISTRY_NAME")
                .or_else(|| env_string("NACOS_NAMING_SERVICE_NAME"))
                .unwrap_or_else(default_service_name),
            group_name: env_string("SERVICE_REGISTRY_GROUP")
                .or_else(|| env_string("NACOS_NAMING_GROUP_NAME"))
                .unwrap_or_else(default_group),
            cluster_name: env_string("SERVICE_REGISTRY_CLUSTER")
                .unwrap_or_else(default_cluster),
            weight: env_string("SERVICE_REGISTRY_WEIGHT")
                .and_then(|s| s.parse().ok())
                .unwrap_or_else(default_weight),
            metadata: HashMap::new(),
            nacos: NacosNamingConfig::from_env(),
        }
    }

    /// 获取服务名称（兼容方法，支持运行时环境变量覆盖）。
    ///
    /// # Returns
    ///
    /// 优先返回 `SERVICE_REGISTRY_NAME` 环境变量值，
    /// 其次 `NACOS_NAMING_SERVICE_NAME`，最后回退到结构体中存储的字段。
    pub fn service_name(&self) -> String {
        env_string("SERVICE_REGISTRY_NAME")
            .or_else(|| env_string("NACOS_NAMING_SERVICE_NAME"))
            .unwrap_or_else(|| self.service_name.clone())
    }

    /// 构建 `ServiceInstance`（便捷方法）。
    ///
    /// 将 `RegistryConfig` 中的通用注册参数组装为 [`ServiceInstance`](super::registry::trait_rs::ServiceInstance)。
    /// IP 和端口需要调用方提供（因为它们来自其他配置源）。
    ///
    /// # Arguments
    ///
    /// * `ip` - 服务实例 IP 地址。
    /// * `port` - 服务实例端口号。
    ///
    /// # Returns
    ///
    /// 返回组装好的 `ServiceInstance`，`healthy` 和 `ephemeral` 默认为 `true`。
    pub fn build_instance(&self, ip: String, port: u16) -> super::registry::trait_rs::ServiceInstance {
        super::registry::trait_rs::ServiceInstance {
            ip,
            port,
            service_name: self.service_name(),
            group_name: Some(self.group_name.clone()),
            cluster_name: Some(self.cluster_name.clone()),
            weight: self.weight,
            metadata: self.metadata.clone(),
            healthy: true,
            ephemeral: true,
        }
    }
}

impl NacosNamingConfig {
    /// 从环境变量加载 Nacos 命名服务配置。
    ///
    /// # 读取的环境变量
    ///
    /// * `NACOS_SERVER_ADDR` - Nacos 服务器地址。
    /// * `NACOS_NAMESPACE` - 命名空间。
    /// * `NACOS_APP_NAME` - 应用名称。
    /// * `NACOS_USERNAME` - 认证用户名（可选）。
    /// * `NACOS_PASSWORD` - 认证密码（可选）。
    ///
    /// # Returns
    ///
    /// 返回从环境变量解析得到的 `NacosNamingConfig`。
    pub fn from_env() -> Self {
        let (server_addr, namespace, app_name, username, password) = nacos_common_from_env();
        Self {
            server_addr,
            namespace,
            app_name,
            username,
            password,
        }
    }
}

impl ConfigCenterFullConfig {
    /// 从环境变量加载配置中心配置。
    ///
    /// # 读取的环境变量
    ///
    /// * `NACOS_ENABLED` / `NACOS_CONFIG_ENABLED` - 控制启用。
    /// * `CONFIG_CENTER_TYPE` - 配置中心类型。
    /// * `CONFIG_CENTER_ENABLED` - 启用标志。
    /// * `NACOS_CONFIG_DATA_ID` - 监听配置项的 data_id（可选）。
    /// * `NACOS_CONFIG_GROUP` - 监听配置项的 group。
    ///
    /// # 兼容性规则
    ///
    /// 当 `NACOS_ENABLED=true` 且 `NACOS_CONFIG_ENABLED=true` 时，
    /// 自动选择 `nacos` 类型并启用配置中心。
    ///
    /// # Returns
    ///
    /// 返回从环境变量解析得到的 `ConfigCenterFullConfig`。
    pub fn from_env() -> Self {
        let nacos_enabled = env_bool("NACOS_ENABLED");
        let config_enabled = env_bool("NACOS_CONFIG_ENABLED");

        // 兼容旧 NACOS_ENABLED：启用时自动设为 nacos 类型。
        let center_type = if nacos_enabled {
            env_string("CONFIG_CENTER_TYPE").unwrap_or_else(|| "nacos".to_string())
        } else {
            env_string("CONFIG_CENTER_TYPE").unwrap_or_else(default_center_type)
        };

        let enabled = if nacos_enabled && config_enabled {
            true
        } else {
            env_bool("CONFIG_CENTER_ENABLED")
        };

        Self {
            center_type,
            enabled,
            nacos: NacosConfigCenterConfig::from_env(),
            listeners: {
                // 仅在设置了 NACOS_CONFIG_DATA_ID 时构造监听项。
                let data_id = env_string("NACOS_CONFIG_DATA_ID");
                let group = env_string("NACOS_CONFIG_GROUP").unwrap_or_else(default_group);
                if let Some(did) = data_id {
                    vec![ConfigListener { data_id: did, group }]
                } else {
                    vec![]
                }
            },
        }
    }
}

impl NacosConfigCenterConfig {
    /// 从环境变量加载 Nacos 配置中心配置。
    ///
    /// # 读取的环境变量
    ///
    /// * `NACOS_SERVER_ADDR` - Nacos 服务器地址。
    /// * `NACOS_NAMESPACE` - 命名空间。
    /// * `NACOS_APP_NAME` - 应用名称。
    /// * `NACOS_USERNAME` - 认证用户名（可选）。
    /// * `NACOS_PASSWORD` - 认证密码（可选）。
    ///
    /// # Returns
    ///
    /// 返回从环境变量解析得到的 `NacosConfigCenterConfig`。
    pub fn from_env() -> Self {
        let (server_addr, namespace, app_name, username, password) = nacos_common_from_env();
        Self {
            server_addr,
            namespace,
            app_name,
            username,
            password,
        }
    }
}

// ============================================================
// 环境变量辅助函数
// ============================================================

/// 读取 Nacos 公共连接配置（Naming 和 ConfigCenter 共用）。
fn nacos_common_from_env() -> (String, String, String, Option<String>, Option<String>) {
    (
        env_string("NACOS_SERVER_ADDR").unwrap_or_else(default_server_addr),
        env_string("NACOS_NAMESPACE").unwrap_or_default(),
        env_string("NACOS_APP_NAME").unwrap_or_else(default_app_name),
        env_string("NACOS_USERNAME"),
        env_string("NACOS_PASSWORD"),
    )
}

/// 读取环境变量字符串值，若未设置或读取失败返回 `None`。
///
/// # Arguments
///
/// * `key` - 环境变量名称。
fn env_string(key: &str) -> Option<String> {
    std::env::var(key).ok()
}

/// 读取布尔型环境变量，识别 `true` / `1`（大小写不敏感）为 `true`，其他为 `false`。
///
/// # Arguments
///
/// * `key` - 环境变量名称。
fn env_bool(key: &str) -> bool {
    std::env::var(key)
        .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
        .unwrap_or(false)
}

/// 读取布尔型环境变量，未设置时返回指定的默认值。
///
/// # Arguments
///
/// * `key` - 环境变量名称。
/// * `default` - 环境变量未设置时返回的默认值。
fn env_bool_or(key: &str, default: bool) -> bool {
    std::env::var(key)
        .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
        .unwrap_or(default)
}
