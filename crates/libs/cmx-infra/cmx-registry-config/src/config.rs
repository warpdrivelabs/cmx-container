//! 配置模型定义
//!
//! 包含注册中心和配置中心的通用配置结构，
//! 支持从环境变量加载，兼容现有 NACOS_* 环境变量。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// ============================================================
// 注册中心配置
// ============================================================

/// 注册中心配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryConfig {
    /// 注册中心类型：mock | nacos | consul | etcd
    #[serde(default = "default_registry_type")]
    pub registry_type: String,

    /// 是否启用服务注册
    #[serde(default)]
    pub enabled: bool,

    /// Nacos 注册中心配置（type = "nacos" 时生效）
    #[serde(default)]
    pub nacos: NacosNamingConfig,
}

/// Nacos 命名服务配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NacosNamingConfig {
    /// Nacos 服务器地址
    #[serde(default = "default_server_addr")]
    pub server_addr: String,

    /// 命名空间
    #[serde(default)]
    pub namespace: String,

    /// 应用名称
    #[serde(default = "default_app_name")]
    pub app_name: String,

    /// 认证用户名
    #[serde(default)]
    pub username: Option<String>,

    /// 认证密码
    #[serde(default)]
    pub password: Option<String>,

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

    /// 实例元数据
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

// ============================================================
// 配置中心配置
// ============================================================

/// 配置中心配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigCenterFullConfig {
    /// 配置中心类型：mock | nacos | apollo
    #[serde(default = "default_center_type")]
    pub center_type: String,

    /// 是否启用配置中心
    #[serde(default)]
    pub enabled: bool,

    /// Nacos 配置中心配置（type = "nacos" 时生效）
    #[serde(default)]
    pub nacos: NacosConfigCenterConfig,

    /// 配置监听列表
    #[serde(default)]
    pub listeners: Vec<ConfigListener>,
}

/// Nacos 配置中心配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NacosConfigCenterConfig {
    /// Nacos 服务器地址
    #[serde(default = "default_server_addr")]
    pub server_addr: String,

    /// 命名空间
    #[serde(default)]
    pub namespace: String,

    /// 应用名称
    #[serde(default = "default_app_name")]
    pub app_name: String,

    /// 认证用户名
    #[serde(default)]
    pub username: Option<String>,

    /// 认证密码
    #[serde(default)]
    pub password: Option<String>,
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

// ============================================================
// 默认值函数
// ============================================================

fn default_registry_type() -> String {
    "mock".to_string()
}

fn default_center_type() -> String {
    "mock".to_string()
}

fn default_server_addr() -> String {
    "127.0.0.1:8848".to_string()
}

fn default_app_name() -> String {
    "cmx-container".to_string()
}

fn default_service_name() -> String {
    "cmx-server".to_string()
}

fn default_group() -> String {
    "DEFAULT_GROUP".to_string()
}

fn default_cluster() -> String {
    "DEFAULT".to_string()
}

fn default_weight() -> f64 {
    1.0
}

// ============================================================
// Default 实现
// ============================================================

impl Default for RegistryConfig {
    fn default() -> Self {
        Self {
            registry_type: default_registry_type(),
            enabled: false,
            nacos: NacosNamingConfig::default(),
        }
    }
}

impl Default for NacosNamingConfig {
    fn default() -> Self {
        Self {
            server_addr: default_server_addr(),
            namespace: String::new(),
            app_name: default_app_name(),
            username: None,
            password: None,
            service_name: default_service_name(),
            group_name: default_group(),
            cluster_name: default_cluster(),
            weight: default_weight(),
            metadata: HashMap::new(),
        }
    }
}

impl Default for ConfigCenterFullConfig {
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
    /// 从环境变量加载注册中心配置
    ///
    /// 支持的环境变量：
    /// - `SERVICE_REGISTRY_TYPE` - 注册中心类型
    /// - `SERVICE_REGISTRY_ENABLED` - 是否启用
    /// - 兼容 `NACOS_*` 环境变量
    pub fn from_env() -> Self {
        let nacos_enabled = env_bool("NACOS_ENABLED");
        let naming_enabled = env_bool_or("NACOS_NAMING_ENABLED", true);

        // 兼容旧 NACOS_ENABLED：启用时自动设为 nacos 类型
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
            nacos: NacosNamingConfig::from_env(),
        }
    }
}

impl NacosNamingConfig {
    /// 从环境变量加载 Nacos 命名服务配置
    pub fn from_env() -> Self {
        Self {
            server_addr: env_string("NACOS_SERVER_ADDR").unwrap_or_else(default_server_addr),
            namespace: env_string("NACOS_NAMESPACE").unwrap_or_default(),
            app_name: env_string("NACOS_APP_NAME").unwrap_or_else(default_app_name),
            username: env_string("NACOS_USERNAME"),
            password: env_string("NACOS_PASSWORD"),
            service_name: env_string("NACOS_NAMING_SERVICE_NAME")
                .unwrap_or_else(default_service_name),
            group_name: env_string("NACOS_NAMING_GROUP_NAME").unwrap_or_else(default_group),
            cluster_name: default_cluster(),
            weight: default_weight(),
            metadata: HashMap::new(),
        }
    }
}

impl ConfigCenterFullConfig {
    /// 从环境变量加载配置中心配置
    pub fn from_env() -> Self {
        let nacos_enabled = env_bool("NACOS_ENABLED");
        let config_enabled = env_bool("NACOS_CONFIG_ENABLED");

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
    /// 从环境变量加载 Nacos 配置中心配置
    pub fn from_env() -> Self {
        Self {
            server_addr: env_string("NACOS_SERVER_ADDR").unwrap_or_else(default_server_addr),
            namespace: env_string("NACOS_NAMESPACE").unwrap_or_default(),
            app_name: env_string("NACOS_APP_NAME").unwrap_or_else(default_app_name),
            username: env_string("NACOS_USERNAME"),
            password: env_string("NACOS_PASSWORD"),
        }
    }
}

// ============================================================
// 环境变量辅助函数
// ============================================================

fn env_string(key: &str) -> Option<String> {
    std::env::var(key).ok()
}

fn env_bool(key: &str) -> bool {
    std::env::var(key)
        .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
        .unwrap_or(false)
}

fn env_bool_or(key: &str, default: bool) -> bool {
    std::env::var(key)
        .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
        .unwrap_or(default)
}
