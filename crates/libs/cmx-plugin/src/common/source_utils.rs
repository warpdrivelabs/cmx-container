//! 插件来源工具模块
//!
//! 提供插件来源类型的解析与构建工具函数，消除各服务间的代码重复。
//!
//! # 功能概述
//!
//! - 从 PluginSource 提取来源类型和地址信息
//! - 根据 zip 来源类型和地址构建 PluginSource

use std::path::PathBuf;

use crate::domain::plugin::PluginSource;

/// 从 PluginSource 提取来源类型和地址信息。
///
/// 返回元组 (来源类型, 地址)，来源类型为 "local"、"remote"、"marketplace" 或 "storage"。
pub fn extract_source_info(source: &PluginSource) -> (Option<String>, Option<String>) {
    match source {
        PluginSource::Local { path } => {
            (Some("local".to_string()), Some(path.to_string_lossy().to_string()))
        }
        PluginSource::Remote { url, .. } => {
            (Some("remote".to_string()), Some(url.clone()))
        }
        PluginSource::Marketplace { marketplace_url, plugin_id } => {
            let url = marketplace_url.as_ref().map(|s| s.as_str()).unwrap_or(plugin_id);
            (Some("marketplace".to_string()), Some(url.to_string()))
        }
        PluginSource::Storage { file_id, .. } => {
            (Some("storage".to_string()), Some(file_id.clone()))
        }
    }
}

/// 根据 zip 来源类型和地址构建 PluginSource。
///
/// 支持的来源类型：local、url/remote、registry/marketplace、storage。
/// 未匹配时默认构建 Local 类型。
pub fn build_plugin_source(zip_source_url: Option<&str>, zip_source_type: Option<&str>) -> PluginSource {
    match zip_source_type {
        Some("local") => {
            let path = zip_source_url.map(PathBuf::from).unwrap_or_default();
            PluginSource::Local { path }
        }
        Some("url") | Some("remote") => {
            let url = zip_source_url.unwrap_or_default().to_string();
            PluginSource::Remote { url, checksum: None }
        }
        Some("registry") | Some("marketplace") => {
            let plugin_id = zip_source_url.unwrap_or_default().to_string();
            PluginSource::Marketplace {
                marketplace_url: None,
                plugin_id,
            }
        }
        Some("storage") => {
            PluginSource::Storage {
                file_id: zip_source_url.unwrap_or_default().to_string(),
                checksum: None,
            }
        }
        _ => {
            let path = zip_source_url.map(PathBuf::from).unwrap_or_default();
            PluginSource::Local { path }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== extract_source_info ====================

    #[test]
    fn test_extract_source_info_local() {
        let source = PluginSource::Local {
            path: PathBuf::from("/tmp/plugin.zip"),
        };
        let (kind, addr) = extract_source_info(&source);
        assert_eq!(kind.as_deref(), Some("local"));
        assert_eq!(addr.as_deref(), Some("/tmp/plugin.zip"));
    }

    #[test]
    fn test_extract_source_info_remote() {
        let source = PluginSource::Remote {
            url: "https://example.com/plugin.zip".to_string(),
            checksum: Some("abc".to_string()),
        };
        let (kind, addr) = extract_source_info(&source);
        assert_eq!(kind.as_deref(), Some("remote"));
        assert_eq!(addr.as_deref(), Some("https://example.com/plugin.zip"));
    }

    #[test]
    fn test_extract_source_info_marketplace_with_url() {
        let source = PluginSource::Marketplace {
            marketplace_url: Some("https://market.example.com".to_string()),
            plugin_id: "my-plugin".to_string(),
        };
        let (kind, addr) = extract_source_info(&source);
        assert_eq!(kind.as_deref(), Some("marketplace"));
        // 当 marketplace_url 存在，地址优先返回 marketplace_url
        assert_eq!(addr.as_deref(), Some("https://market.example.com"));
    }

    #[test]
    fn test_extract_source_info_marketplace_url_none_falls_back_to_plugin_id() {
        let source = PluginSource::Marketplace {
            marketplace_url: None,
            plugin_id: "my-plugin".to_string(),
        };
        let (kind, addr) = extract_source_info(&source);
        assert_eq!(kind.as_deref(), Some("marketplace"));
        // 当 marketplace_url 为 None，地址回退到 plugin_id
        assert_eq!(addr.as_deref(), Some("my-plugin"));
    }

    #[test]
    fn test_extract_source_info_storage() {
        let source = PluginSource::Storage {
            file_id: "file-123".to_string(),
            checksum: Some("xyz".to_string()),
        };
        let (kind, addr) = extract_source_info(&source);
        assert_eq!(kind.as_deref(), Some("storage"));
        assert_eq!(addr.as_deref(), Some("file-123"));
    }

    // ==================== build_plugin_source ====================

    #[test]
    fn test_build_plugin_source_local() {
        let source = build_plugin_source(Some("/tmp/plugin.zip"), Some("local"));
        assert!(matches!(source, PluginSource::Local { path } if path.as_path() == std::path::Path::new("/tmp/plugin.zip")));
    }

    #[test]
    fn test_build_plugin_source_local_without_url_returns_empty_path() {
        let source = build_plugin_source(None, Some("local"));
        assert!(matches!(source, PluginSource::Local { path } if path.as_os_str().is_empty()));
    }

    #[test]
    fn test_build_plugin_source_remote_alias_url() {
        // "url" 与 "remote" 均应构建 Remote 来源
        let from_url = build_plugin_source(Some("https://example.com/a.zip"), Some("url"));
        let from_remote = build_plugin_source(Some("https://example.com/a.zip"), Some("remote"));
        assert!(matches!(from_url, PluginSource::Remote { url, checksum: None }
            if url == "https://example.com/a.zip"));
        assert!(matches!(from_remote, PluginSource::Remote { url, checksum: None }
            if url == "https://example.com/a.zip"));
    }

    #[test]
    fn test_build_plugin_source_remote_url_none_returns_empty() {
        let source = build_plugin_source(None, Some("remote"));
        assert!(matches!(source, PluginSource::Remote { url, checksum: None } if url.is_empty()));
    }

    #[test]
    fn test_build_plugin_source_marketplace_alias_registry() {
        // "registry" 与 "marketplace" 均应构建 Marketplace 来源
        let from_registry = build_plugin_source(Some("pid-1"), Some("registry"));
        let from_market = build_plugin_source(Some("pid-1"), Some("marketplace"));
        assert!(matches!(from_registry, PluginSource::Marketplace { marketplace_url: None, plugin_id }
            if plugin_id == "pid-1"));
        assert!(matches!(from_market, PluginSource::Marketplace { marketplace_url: None, plugin_id }
            if plugin_id == "pid-1"));
    }

    #[test]
    fn test_build_plugin_source_storage() {
        let source = build_plugin_source(Some("fid-9"), Some("storage"));
        assert!(matches!(source, PluginSource::Storage { file_id, checksum: None }
            if file_id == "fid-9"));
    }

    #[test]
    fn test_build_plugin_source_unknown_type_defaults_to_local() {
        // 未知类型应回退为 Local，且使用 url 作为 path
        let source = build_plugin_source(Some("/some/path"), Some("unknown_type"));
        assert!(matches!(source, PluginSource::Local { path } if path.as_path() == std::path::Path::new("/some/path")));
    }

    #[test]
    fn test_build_plugin_source_none_type_defaults_to_local() {
        // 类型为 None 时也应回退为 Local
        let source = build_plugin_source(Some("/some/path"), None);
        assert!(matches!(source, PluginSource::Local { path } if path.as_path() == std::path::Path::new("/some/path")));
    }

    #[test]
    fn test_build_plugin_source_none_type_none_url_returns_empty_local() {
        let source = build_plugin_source(None, None);
        assert!(matches!(source, PluginSource::Local { path } if path.as_os_str().is_empty()));
    }
}
