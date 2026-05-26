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
