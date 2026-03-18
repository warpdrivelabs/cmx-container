//! 插件配置模块 - TOML 配置文件解析
//!
//! 提供系统默认插件配置的 TOML 文件解析功能。

use std::path::Path;

use serde::{Deserialize, Serialize};

/// 系统插件配置文件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemPluginsConfig {
    /// 配置设置
    #[serde(default)]
    pub settings: PluginSettings,
    /// 必需插件列表
    #[serde(default)]
    pub required: Vec<RequiredPlugin>,
    /// 可选插件列表
    #[serde(default)]
    pub optional: Vec<OptionalPlugin>,
}

/// 插件设置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginSettings {
    /// 插件安装根目录
    #[serde(default = "default_install_root")]
    pub install_root: String,
    /// 默认数据库 ID
    #[serde(default = "default_db_id")]
    pub default_db_id: String,
    /// 最大并发安装数
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent: usize,
    /// 安装超时时间（秒）
    #[serde(default = "default_install_timeout")]
    pub install_timeout: u64,
    /// 是否启用备份
    #[serde(default = "default_enable_backup")]
    pub enable_backup: bool,
    /// 备份保留数量
    #[serde(default = "default_backup_count")]
    pub backup_count: usize,
}

impl Default for PluginSettings {
    fn default() -> Self {
        Self {
            install_root: default_install_root(),
            default_db_id: default_db_id(),
            max_concurrent: default_max_concurrent(),
            install_timeout: default_install_timeout(),
            enable_backup: default_enable_backup(),
            backup_count: default_backup_count(),
        }
    }
}

fn default_install_root() -> String { "plugins/".to_string() }
fn default_db_id() -> String { "default".to_string() }
fn default_max_concurrent() -> usize { 3 }
fn default_install_timeout() -> u64 { 300 }
fn default_enable_backup() -> bool { true }
fn default_backup_count() -> usize { 5 }

/// 必需插件配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequiredPlugin {
    /// 插件 ID
    pub id: String,
    /// 版本约束
    pub version: String,
    /// 插件来源
    pub source: PluginSourceConfig,
    /// 安装顺序
    #[serde(default)]
    pub order: i32,
    /// 重试次数
    #[serde(default = "default_retry_count")]
    pub retry_count: u32,
    /// 回退版本
    pub fallback_version: Option<String>,
}

/// 可选插件配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptionalPlugin {
    /// 插件 ID
    pub id: String,
    /// 版本约束
    pub version: String,
    /// 插件来源
    pub source: PluginSourceConfig,
    /// 安装顺序
    #[serde(default)]
    pub order: i32,
    /// 重试次数
    #[serde(default = "default_retry_count")]
    pub retry_count: u32,
    /// 是否默认启用
    #[serde(default)]
    pub enabled_by_default: bool,
}

fn default_retry_count() -> u32 { 3 }

/// 插件来源配置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum PluginSourceConfig {
    /// ZIP 文件
    #[serde(rename = "zip")]
    Zip { path: String },
    /// URL 下载
    #[serde(rename = "url")]
    Url { url: String },
    /// 注册表
    #[serde(rename = "registry")]
    Registry { name: String, version: Option<String> },
    /// 目录
    #[serde(rename = "directory")]
    Directory { path: String },
}

impl SystemPluginsConfig {
    /// 从文件加载配置
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path.as_ref())
            .map_err(|e| ConfigError::Io(e.to_string()))?;
        
        Self::from_str(&content)
    }

    /// 从字符串解析配置
    pub fn from_str(content: &str) -> Result<Self, ConfigError> {
        toml::from_str(content)
            .map_err(|e| ConfigError::Parse(e.to_string()))
    }

    /// 获取所有插件（按顺序）
    pub fn get_all_plugins_ordered(&self) -> Vec<(bool, &str, &str, &PluginSourceConfig)> {
        let mut plugins: Vec<(bool, &str, &str, &PluginSourceConfig, i32)> = Vec::new();
        
        // 添加必需插件
        for plugin in &self.required {
            plugins.push((true, plugin.id.as_str(), plugin.version.as_str(), &plugin.source, plugin.order));
        }
        
        // 添加可选插件
        for plugin in &self.optional {
            plugins.push((false, plugin.id.as_str(), plugin.version.as_str(), &plugin.source, plugin.order));
        }
        
        // 按顺序排序
        plugins.sort_by_key(|p| p.4);
        
        // 返回结果
        plugins.into_iter()
            .map(|(required, id, version, source, _)| (required, id, version, source))
            .collect()
    }

    /// 获取必需插件列表
    pub fn get_required_plugins(&self) -> &[RequiredPlugin] {
        &self.required
    }

    /// 获取可选插件列表
    pub fn get_optional_plugins(&self) -> &[OptionalPlugin] {
        &self.optional
    }
}

/// 配置错误
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("IO 错误: {0}")]
    Io(String),
    #[error("解析错误: {0}")]
    Parse(String),
    #[error("验证错误: {0}")]
    Validation(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_config() {
        let toml_content = r#"
[settings]
install_root = "plugins/"
default_db_id = "default"
max_concurrent = 3

[[required]]
id = "cmx-core-tables"
version = "^1.0.0"
source = { type = "zip", path = "plugins/packages/cmx-core-tables-1.0.0.zip" }
order = 1

[[required]]
id = "cmx-auth"
version = "^1.0.0"
source = { type = "registry", name = "cmx-auth" }
order = 2

[[optional]]
id = "cmx-reporting"
version = "^1.0.0"
source = { type = "url", url = "https://plugins.example.com/reporting.zip" }
enabled_by_default = false
"#;

        let config = SystemPluginsConfig::from_str(toml_content).unwrap();
        
        assert_eq!(config.settings.install_root, "plugins/");
        assert_eq!(config.required.len(), 2);
        assert_eq!(config.optional.len(), 1);
        
        let plugins = config.get_all_plugins_ordered();
        assert_eq!(plugins.len(), 3);
        assert_eq!(plugins[0].1, "cmx-core-tables");
        assert_eq!(plugins[1].1, "cmx-auth");
        assert_eq!(plugins[2].1, "cmx-reporting");
    }
}
