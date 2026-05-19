//! Nacos 配置模型定义
//!
//! 包含 Nacos 连接配置、命名服务配置、配置中心配置等结构体

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Nacos 集成配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NacosConfig {
    /// Nacos 服务器地址（如 "127.0.0.1:8848"）
    #[serde(default = "default_server_addr")]
    pub server_addr: String,

    /// 命名空间（默认 ""，即 public）
    #[serde(default)]
    pub namespace: String,

    /// 应用名称
    #[serde(default = "default_app_name")]
    pub app_name: String,

    /// 认证用户名（可选）
    #[serde(default)]
    pub username: Option<String>,

    /// 认证密码（可选）
    #[serde(default)]
    pub password: Option<String>,

    /// 是否启用 Nacos 集成
    #[serde(default)]
    pub enabled: bool,

    /// 服务注册配置
    #[serde(default)]
    pub naming: NamingConfig,

    /// 配置中心配置
    #[serde(default)]
    pub config: ConfigCenterConfig,
}

/// 服务注册配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamingConfig {
    /// 服务名称
    #[serde(default = "default_service_name")]
    pub service_name: String,

    /// 分组名称
    #[serde(default = "default_group")]
    pub group_name: String,

    /// 集群名称
    #[serde(default = "default_cluster")]
    pub cluster_name: String,

    /// 实例权重
    #[serde(default = "default_weight")]
    pub weight: f64,

    /// 是否启用命名服务
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// 实例元数据
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

/// 配置中心配置
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConfigCenterConfig {
    /// 是否启用远程配置
    #[serde(default)]
    pub enabled: bool,

    /// 配置监听列表
    #[serde(default)]
    pub listeners: Vec<ConfigListener>,
}

/// 配置监听项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigListener {
    /// Data ID（配置标识）
    pub data_id: String,

    /// Group（配置分组）
    #[serde(default = "default_group")]
    pub group: String,
}

/// 默认服务器地址
fn default_server_addr() -> String {
    "127.0.0.1:8848".to_string()
}

/// 默认应用名称
fn default_app_name() -> String {
    "cmx-container".to_string()
}

/// 默认服务名称
fn default_service_name() -> String {
    "cmx-server".to_string()
}

/// 默认分组
fn default_group() -> String {
    "DEFAULT_GROUP".to_string()
}

/// 默认集群名称
fn default_cluster() -> String {
    "DEFAULT".to_string()
}

/// 默认权重
fn default_weight() -> f64 {
    1.0
}

/// 默认布尔值 true
fn default_true() -> bool {
    true
}

impl Default for NacosConfig {
    fn default() -> Self {
        Self {
            server_addr: default_server_addr(),
            namespace: String::new(),
            app_name: default_app_name(),
            username: None,
            password: None,
            enabled: false,
            naming: NamingConfig::default(),
            config: ConfigCenterConfig::default(),
        }
    }
}

impl Default for NamingConfig {
    fn default() -> Self {
        Self {
            service_name: default_service_name(),
            group_name: default_group(),
            cluster_name: default_cluster(),
            weight: default_weight(),
            enabled: true,
            metadata: HashMap::new(),
        }
    }
}

impl NacosConfig {
    /// 从环境变量加载 Nacos 配置
    ///
    /// 环境变量命名规则：`NACOS_` 前缀 + 大写键名，用 `__` 作为层级分隔符。
    ///
    /// 支持的环境变量：
    /// - `NACOS_ENABLED` - 是否启用（"true"/"false"）
    /// - `NACOS_SERVER_ADDR` - 服务器地址
    /// - `NACOS_NAMESPACE` - 命名空间
    /// - `NACOS_APP_NAME` - 应用名称
    /// - `NACOS_USERNAME` - 认证用户名（可选）
    /// - `NACOS_PASSWORD` - 认证密码（可选）
    /// - `NACOS_NAMING_ENABLED` - 是否启用服务注册
    /// - `NACOS_NAMING_SERVICE_NAME` - 服务名称
    /// - `NACOS_NAMING_GROUP_NAME` - 分组名称
    /// - `NACOS_CONFIG_ENABLED` - 是否启用配置中心
    /// - `NACOS_CONFIG_DATA_ID` - 配置 Data ID
    /// - `NACOS_CONFIG_GROUP` - 配置 Group
    pub fn from_env() -> Self {
        Self {
            enabled: env_bool("NACOS_ENABLED"),
            server_addr: env_string("NACOS_SERVER_ADDR").unwrap_or_else(default_server_addr),
            namespace: env_string("NACOS_NAMESPACE").unwrap_or_default(),
            app_name: env_string("NACOS_APP_NAME").unwrap_or_else(default_app_name),
            username: env_string("NACOS_USERNAME"),
            password: env_string("NACOS_PASSWORD"),
            naming: NamingConfig {
                enabled: env_bool_or("NACOS_NAMING_ENABLED", true),
                service_name: env_string("NACOS_NAMING_SERVICE_NAME")
                    .unwrap_or_else(default_service_name),
                group_name: env_string("NACOS_NAMING_GROUP_NAME")
                    .unwrap_or_else(default_group),
                cluster_name: default_cluster(),
                weight: default_weight(),
                metadata: HashMap::new(),
            },
            config: ConfigCenterConfig {
                enabled: env_bool_or("NACOS_CONFIG_ENABLED", false),
                listeners: {
                    let data_id = env_string("NACOS_CONFIG_DATA_ID");
                    let group = env_string("NACOS_CONFIG_GROUP")
                        .unwrap_or_else(default_group);
                    if let Some(did) = data_id {
                        vec![ConfigListener { data_id: did, group }]
                    } else {
                        vec![]
                    }
                },
            },
        }
    }
}

/// 从环境变量读取字符串值
fn env_string(key: &str) -> Option<String> {
    std::env::var(key).ok()
}

/// 从环境变量读取布尔值（默认 false）
fn env_bool(key: &str) -> bool {
    std::env::var(key)
        .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
        .unwrap_or(false)
}

/// 从环境变量读取布尔值（带默认值）
fn env_bool_or(key: &str, default: bool) -> bool {
    std::env::var(key)
        .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
        .unwrap_or(default)
}
