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
    /// 插件市场。
    Marketplace {
        /// 市场服务地址。
        marketplace_url: String,
        /// 插件ID。
        plugin_id: String,
        /// 版本约束。
        version_constraint: Option<String>,
    },
    /// cmx-storage 存储
    Storage {
        /// 文件唯一标识
        file_id: String,
        /// 校验和
        checksum: Option<String>,
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

    /// 创建插件市场来源。
    pub fn marketplace(marketplace_url: String, plugin_id: String, version_constraint: Option<String>) -> Self {
        Self::Marketplace {
            marketplace_url,
            plugin_id,
            version_constraint,
        }
    }

    /// 创建存储来源
    pub fn storage(file_id: String, checksum: Option<String>) -> Self {
        Self::Storage { file_id, checksum }
    }
    
    /// 检查是否为本地来源
    pub fn is_local(&self) -> bool {
        matches!(self, PluginSource::Local { .. })
    }
    
    /// 检查是否为远程来源
    pub fn is_remote(&self) -> bool {
        matches!(self, PluginSource::Remote { .. })
    }

    /// 检查是否为插件市场来源。
    pub fn is_marketplace(&self) -> bool {
        matches!(self, PluginSource::Marketplace { .. })
    }

    /// 检查是否为存储来源
    pub fn is_storage(&self) -> bool {
        matches!(self, PluginSource::Storage { .. })
    }
}

/// 来源类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceType {
    /// 本地
    Local,
    /// 远程
    Remote,
    /// 插件市场。
    Marketplace,
    /// 存储
    Storage,
}

impl PluginSource {
    /// 获取来源类型
    pub fn source_type(&self) -> SourceType {
        match self {
            PluginSource::Local { .. } => SourceType::Local,
            PluginSource::Remote { .. } => SourceType::Remote,
            PluginSource::Marketplace { .. } => SourceType::Marketplace,
            PluginSource::Storage { .. } => SourceType::Storage,
        }
    }
}
