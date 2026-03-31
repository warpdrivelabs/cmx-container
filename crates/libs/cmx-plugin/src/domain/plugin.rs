//! 插件定义与信息模块
//!
//! 定义插件的核心数据结构

use std::path::PathBuf;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 插件信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfo {
    /// 插件ID
    pub id: String,
    /// 插件名称
    pub name: String,
    /// 插件版本
    pub version: String,
    /// 插件描述
    pub description: Option<String>,
    /// 插件作者
    pub author: Option<String>,
    /// 插件来源
    pub source: PluginSource,
    /// 插件状态
    pub status: PluginStatus,
    /// 安装时间
    pub installed_at: Option<chrono::DateTime<chrono::Utc>>,
    /// 更新时间
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
    /// 安装路径
    pub install_path: PathBuf,
    /// 插件类型 (wasm/rhai)
    pub plugin_type: String,
    /// 源码路径
    pub source_path: Option<String>,
    // 插件域编码
    pub domain_code: String,
    /// 应用编码
    pub application_code: String,
    /// 模块编码
    pub module_code: String,

    /// 创建时间
    pub create_time: DateTime<Utc>,
    /// 更新时间
    pub update_time: DateTime<Utc>,
    /// 创建人ID
    pub create_by: Option<String>,
    /// 创建人名称
    pub create_name: Option<String>,
    /// 更新人ID
    pub update_by: Option<String>,
    /// 更新人名称
    pub update_name: Option<String>,
}

/// 插件来源
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PluginSource {
    /// 本地文件
    Local {
        path: PathBuf,
    },
    /// 远程URL
    Remote {
        url: String,
        checksum: Option<String>,
    },
    /// 远程注册表，可以认为是插件市场？
    Registry {
        registry_url: Option<String>,
        package_name: String,
    },
}

/// 插件状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
// 允许前端传 "installed", "activated" 等
#[serde(rename_all = "lowercase")]
pub enum PluginStatus {
    /// 已安装
    Installed,
    /// 已激活
    Activated,
    /// 已停用
    Deactivated,
    /// 错误
    Error,
}

impl std::fmt::Display for PluginStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PluginStatus::Installed => write!(f, "installed"),
            PluginStatus::Activated => write!(f, "activated"),
            PluginStatus::Deactivated => write!(f, "deactivated"),
            PluginStatus::Error => write!(f, "error"),
        }
    }
}

/// 从字符串解析插件状态
impl std::str::FromStr for PluginStatus {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "installed" => Ok(PluginStatus::Installed),
            "activated" => Ok(PluginStatus::Activated),
            "deactivated" => Ok(PluginStatus::Deactivated),
            "error" => Ok(PluginStatus::Error),
            _ => Err(format!("未知插件状态: {}", s)),
        }
    }
}


/// 插件筛选条件
#[derive(Debug, Clone, Default,Serialize,Deserialize)]
pub struct PluginFilter {
    /// 按状态筛选
    pub status: Option<PluginStatus>,
    /// 按名称筛选（模糊匹配）
    pub name: Option<String>,
    /// 按域编码筛选
    pub domain_code: Option<String>,
    /// 按应用编码筛选
    pub application_code: Option<String>,
    /// 按模块编码筛选
    pub module_code: Option<String>,
}

/// 插件配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginConfig {
    /// 配置键值对
    pub settings: std::collections::HashMap<String, serde_json::Value>,
}

/// 插件数据库配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginDatabaseConfig {
    /// 数据库ID
    pub db_id: String,
    /// 是否创建独立数据库
    pub create_database: bool,
    /// 表配置文件路径列表
    pub table_config_files: Vec<String>,
}
