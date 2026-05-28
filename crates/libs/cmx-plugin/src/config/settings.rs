//! 配置设置模块
//! 
//! 定义配置结构

use std::collections::HashMap;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};

fn default_app_id() -> String {
    "default".to_string()
}

fn default_reconciliation_interval() -> u64 {
    60
}

/// 插件管理器配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManagerSettings {
    /// 插件安装根目录
    pub plugin_root: PathBuf,
    /// 备份目录
    pub backup_root: PathBuf,
    /// 临时目录
    pub temp_root: PathBuf,
    /// 默认数据库ID
    pub default_database_id: String,
    /// 最大备份数量
    pub max_backups_per_plugin: usize,
    /// 是否启用签名验证
    pub verify_signatures: bool,
    /// 是否启用权限检查
    pub check_permissions: bool,
    /// 缓存配置
    pub cache: CacheSettings,
    /// 集群配置
    pub cluster: Option<ClusterSettings>,
    /// 节点ID（可选，用于审计日志追踪，默认自动生成）
    pub node_id: Option<String>,
    /// 节点名称
    pub node_name: Option<String>,
    /// 节点类型
    pub node_type: Option<String>,
    /// 应用ID
    #[serde(default = "default_app_id")]
    pub app_id: String,
    /// 一致性校验间隔（秒），0 表示禁用定时校验
    #[serde(default = "default_reconciliation_interval")]
    pub reconciliation_interval_secs: u64,
    /// 自动安装配置
    #[serde(default)]
    pub auto_install: crate::service::auto_install::AutoInstallConfig,
}

impl Default for PluginManagerSettings {
    fn default() -> Self {
        Self {
            plugin_root: PathBuf::from("./plugins"),
            backup_root: PathBuf::from("./backups"),
            temp_root: PathBuf::from("./temp"),
            default_database_id: "default".to_string(),
            max_backups_per_plugin: 5,
            verify_signatures: true,
            check_permissions: true,
            cache: CacheSettings::default(),
            cluster: None,
            node_id: None,
            node_name: None,
            node_type: None,
            app_id: default_app_id(),
            reconciliation_interval_secs: default_reconciliation_interval(),
            auto_install: crate::service::auto_install::AutoInstallConfig::default(),
        }
    }
}

/// 缓存配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheSettings {
    /// 是否启用内存缓存
    pub enable_memory_cache: bool,
    /// 是否启用Redis缓存
    pub enable_redis_cache: bool,
    /// 内存缓存TTL（秒）
    pub memory_cache_ttl_seconds: u64,
    /// Redis缓存TTL（秒）
    pub redis_cache_ttl_seconds: u64,
    /// Redis连接URL
    pub redis_url: Option<String>,
}

impl Default for CacheSettings {
    fn default() -> Self {
        Self {
            enable_memory_cache: true,
            enable_redis_cache: false,
            memory_cache_ttl_seconds: 300,
            redis_cache_ttl_seconds: 3600,
            redis_url: None,
        }
    }
}

/// 集群配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterSettings {
    /// 当前节点ID
    pub node_id: String,
    /// 节点名称
    pub node_name: String,
    /// 节点地址
    pub node_address: String,
    /// 心跳间隔（秒）
    pub heartbeat_interval_seconds: u64,
    /// 节点超时时间（秒）
    pub node_timeout_seconds: u64,
}

/// 插件配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginSettings {
    /// 插件ID
    pub plugin_id: String,
    /// 是否启用
    pub enabled: bool,
    /// 配置项
    pub settings: HashMap<String, serde_json::Value>,
    /// 权限列表
    pub permissions: Vec<String>,
}

impl PluginSettings {
    /// 创建新的插件配置
    pub fn new(plugin_id: String) -> Self {
        Self {
            plugin_id,
            enabled: true,
            settings: HashMap::new(),
            permissions: Vec::new(),
        }
    }
    
    /// 设置配置项
    pub fn set(&mut self, key: String, value: serde_json::Value) {
        self.settings.insert(key, value);
    }
    
    /// 获取配置项
    pub fn get(&self, key: &str) -> Option<&serde_json::Value> {
        self.settings.get(key)
    }
}
