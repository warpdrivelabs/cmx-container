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

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== 构造函数 ====================

    #[test]
    fn test_local_constructor() {
        let src = PluginSource::local(PathBuf::from("/tmp/a.zip"));
        assert!(matches!(src, PluginSource::Local { path } if path == PathBuf::from("/tmp/a.zip")));
    }

    #[test]
    fn test_remote_constructor_with_checksum() {
        let src = PluginSource::remote("https://example.com/a.zip".to_string(), Some("ck".to_string()));
        assert!(matches!(src, PluginSource::Remote { url, checksum }
            if url == "https://example.com/a.zip" && checksum.as_deref() == Some("ck")));
    }

    #[test]
    fn test_remote_constructor_without_checksum() {
        let src = PluginSource::remote("https://example.com/a.zip".to_string(), None);
        assert!(matches!(src, PluginSource::Remote { checksum: None, .. }));
    }

    #[test]
    fn test_marketplace_constructor_with_constraint() {
        let src = PluginSource::marketplace(
            "https://market.example.com".to_string(),
            "my-plugin".to_string(),
            Some("^1.0.0".to_string()),
        );
        assert!(matches!(src, PluginSource::Marketplace { marketplace_url, plugin_id, version_constraint }
            if marketplace_url == "https://market.example.com"
            && plugin_id == "my-plugin"
            && version_constraint.as_deref() == Some("^1.0.0")));
    }

    #[test]
    fn test_storage_constructor() {
        let src = PluginSource::storage("fid-1".to_string(), None);
        assert!(matches!(src, PluginSource::Storage { file_id, checksum: None } if file_id == "fid-1"));
    }

    // ==================== is_local / is_remote / is_marketplace / is_storage ====================

    #[test]
    fn test_is_local_true_only_for_local() {
        let local = PluginSource::local(PathBuf::from("/x"));
        assert!(local.is_local());
        assert!(!local.is_remote());
        assert!(!local.is_marketplace());
        assert!(!local.is_storage());
    }

    #[test]
    fn test_is_remote_true_only_for_remote() {
        let remote = PluginSource::remote("https://x".to_string(), None);
        assert!(!remote.is_local());
        assert!(remote.is_remote());
        assert!(!remote.is_marketplace());
        assert!(!remote.is_storage());
    }

    #[test]
    fn test_is_marketplace_true_only_for_marketplace() {
        let market = PluginSource::marketplace("https://m".to_string(), "pid".to_string(), None);
        assert!(!market.is_local());
        assert!(!market.is_remote());
        assert!(market.is_marketplace());
        assert!(!market.is_storage());
    }

    #[test]
    fn test_is_storage_true_only_for_storage() {
        let storage = PluginSource::storage("fid".to_string(), None);
        assert!(!storage.is_local());
        assert!(!storage.is_remote());
        assert!(!storage.is_marketplace());
        assert!(storage.is_storage());
    }

    // ==================== source_type ====================

    #[test]
    fn test_source_type_local() {
        assert_eq!(PluginSource::local(PathBuf::from("/x")).source_type(), SourceType::Local);
    }

    #[test]
    fn test_source_type_remote() {
        assert_eq!(
            PluginSource::remote("https://x".to_string(), None).source_type(),
            SourceType::Remote
        );
    }

    #[test]
    fn test_source_type_marketplace() {
        assert_eq!(
            PluginSource::marketplace("https://m".to_string(), "pid".to_string(), None).source_type(),
            SourceType::Marketplace
        );
    }

    #[test]
    fn test_source_type_storage() {
        assert_eq!(
            PluginSource::storage("fid".to_string(), None).source_type(),
            SourceType::Storage
        );
    }
}
