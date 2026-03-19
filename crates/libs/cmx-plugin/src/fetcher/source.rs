//! 来源定义模块
//! 
//! 定义插件来源类型

use std::path::PathBuf;
use serde::{Deserialize, Serialize};

/// 插件来源
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PluginSource {
    /// 本地文件
    Local {
        /// 文件路径
        path: PathBuf,
    },
    /// 远程URL
    Remote {
        /// URL地址
        url: String,
        /// 校验和
        checksum: Option<String>,
    },
    /// 注册表
    Registry {
        /// 注册表URL
        registry_url: String,
        /// 包名
        package_name: String,
        /// 版本约束
        version_constraint: Option<String>,
    },
}

impl PluginSource {
    /// 创建本地来源
    pub fn local(path: PathBuf) -> Self {
        Self::Local { path }
    }
    
    /// 创建远程来源
    pub fn remote(url: String, checksum: Option<String>) -> Self {
        Self::Remote { url, checksum }
    }
    
    /// 创建注册表来源
    pub fn registry(registry_url: String, package_name: String, version_constraint: Option<String>) -> Self {
        Self::Registry {
            registry_url,
            package_name,
            version_constraint,
        }
    }
    
    /// 检查是否为本地来源
    pub fn is_local(&self) -> bool {
        matches!(self, PluginSource::Local { .. })
    }
    
    /// 检查是否为远程来源
    pub fn is_remote(&self) -> bool {
        matches!(self, PluginSource::Remote { .. })
    }
    
    /// 检查是否为注册表来源
    pub fn is_registry(&self) -> bool {
        matches!(self, PluginSource::Registry { .. })
    }
}

/// 来源类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceType {
    /// 本地
    Local,
    /// 远程
    Remote,
    /// 注册表
    Registry,
}

impl PluginSource {
    /// 获取来源类型
    pub fn source_type(&self) -> SourceType {
        match self {
            PluginSource::Local { .. } => SourceType::Local,
            PluginSource::Remote { .. } => SourceType::Remote,
            PluginSource::Registry { .. } => SourceType::Registry,
        }
    }
}
