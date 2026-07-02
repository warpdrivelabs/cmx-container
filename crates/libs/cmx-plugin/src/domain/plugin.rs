//! 插件定义与信息模块
//!
//! 定义插件的核心数据结构

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

fn default_app_id() -> String {
    "default".to_string()
}

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
    /// 应用ID
    #[serde(default = "default_app_id")]
    pub app_id: String,
    // /// 创建时间
    // pub create_time: DateTime<Utc>,
    // /// 更新时间
    // pub update_time: DateTime<Utc>,
    // /// 创建人ID
    // pub create_by: Option<String>,
    // /// 创建人名称
    // pub create_name: Option<String>,
    // /// 更新人ID
    // pub update_by: Option<String>,
    // /// 更新人名称
    // pub update_name: Option<String>,
}

/// 插件来源
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PluginSource {
    /// 本地文件
    Local { path: PathBuf },
    /// 远程URL
    Remote {
        url: String,
        checksum: Option<String>,
    },
    /// 插件市场。
    Marketplace {
        /// 市场服务地址。
        marketplace_url: Option<String>,
        /// 插件业务 ID。
        plugin_id: String,
    },
    /// cmx-storage 存储
    Storage {
        file_id: String,
        checksum: Option<String>,
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
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginFilter {
    /// 按应用ID筛选
    pub app_id: Option<String>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    // ==================== PluginStatus Display ====================

    #[test]
    fn test_plugin_status_display_installed() {
        assert_eq!(PluginStatus::Installed.to_string(), "installed");
    }

    #[test]
    fn test_plugin_status_display_activated() {
        assert_eq!(PluginStatus::Activated.to_string(), "activated");
    }

    #[test]
    fn test_plugin_status_display_deactivated() {
        assert_eq!(PluginStatus::Deactivated.to_string(), "deactivated");
    }

    #[test]
    fn test_plugin_status_display_error() {
        assert_eq!(PluginStatus::Error.to_string(), "error");
    }

    // ==================== PluginStatus FromStr ====================

    #[test]
    fn test_plugin_status_from_str_installed() {
        let status = PluginStatus::from_str("installed").unwrap();
        assert_eq!(status, PluginStatus::Installed);
    }

    #[test]
    fn test_plugin_status_from_str_activated() {
        let status = PluginStatus::from_str("activated").unwrap();
        assert_eq!(status, PluginStatus::Activated);
    }

    #[test]
    fn test_plugin_status_from_str_deactivated() {
        let status = PluginStatus::from_str("deactivated").unwrap();
        assert_eq!(status, PluginStatus::Deactivated);
    }

    #[test]
    fn test_plugin_status_from_str_error() {
        let status = PluginStatus::from_str("error").unwrap();
        assert_eq!(status, PluginStatus::Error);
    }

    #[test]
    fn test_plugin_status_from_str_case_insensitive() {
        // 大小写不敏感：均应解析成功
        assert_eq!(
            PluginStatus::from_str("INSTALLED").unwrap(),
            PluginStatus::Installed
        );
        assert_eq!(
            PluginStatus::from_str("Activated").unwrap(),
            PluginStatus::Activated
        );
        assert_eq!(
            PluginStatus::from_str("Deactivated").unwrap(),
            PluginStatus::Deactivated
        );
        assert_eq!(
            PluginStatus::from_str("ERROR").unwrap(),
            PluginStatus::Error
        );
    }

    #[test]
    fn test_plugin_status_from_str_unknown_returns_err() {
        let result = PluginStatus::from_str("unknown");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("未知插件状态"),
            "错误消息应包含未知状态提示: {}",
            err
        );
        assert!(err.contains("unknown"), "错误消息应包含原状态值: {}", err);
    }

    #[test]
    fn test_plugin_status_from_str_empty_returns_err() {
        let result = PluginStatus::from_str("");
        assert!(result.is_err());
    }

    // ==================== PluginStatus 序列化 / 反序列化 ====================

    #[test]
    fn test_plugin_status_serde_roundtrip() {
        for status in [
            PluginStatus::Installed,
            PluginStatus::Activated,
            PluginStatus::Deactivated,
            PluginStatus::Error,
        ] {
            let json = serde_json::to_string(&status).unwrap();
            let deserialized: PluginStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(status, deserialized, "状态 {} 应能往返序列化", status);
        }
    }

    #[test]
    fn test_plugin_status_serde_lowercase_format() {
        // serde rename_all = "lowercase" 确保小写输出
        let json = serde_json::to_string(&PluginStatus::Installed).unwrap();
        assert_eq!(json, "\"installed\"");
        let json = serde_json::to_string(&PluginStatus::Error).unwrap();
        assert_eq!(json, "\"error\"");
    }

    // ==================== PluginFilter Default ====================

    #[test]
    fn test_plugin_filter_default_all_none() {
        let filter = PluginFilter::default();
        assert!(filter.app_id.is_none());
        assert!(filter.status.is_none());
        assert!(filter.name.is_none());
        assert!(filter.domain_code.is_none());
        assert!(filter.application_code.is_none());
        assert!(filter.module_code.is_none());
    }

    // ==================== PluginSource 构造与匹配 ====================

    #[test]
    fn test_plugin_source_local_construction() {
        let src = PluginSource::Local {
            path: PathBuf::from("/tmp/plugin"),
        };
        match src {
            PluginSource::Local { path } => assert_eq!(path, PathBuf::from("/tmp/plugin")),
            _ => panic!("应匹配 Local 变体"),
        }
    }

    #[test]
    fn test_plugin_source_remote_with_checksum() {
        let src = PluginSource::Remote {
            url: "https://example.com/a.zip".to_string(),
            checksum: Some("sha256:abc".to_string()),
        };
        match src {
            PluginSource::Remote { url, checksum } => {
                assert_eq!(url, "https://example.com/a.zip");
                assert_eq!(checksum.as_deref(), Some("sha256:abc"));
            }
            _ => panic!("应匹配 Remote 变体"),
        }
    }

    #[test]
    fn test_plugin_source_marketplace_with_optional_url() {
        // marketplace_url 为 Option，允许为 None
        let with_url = PluginSource::Marketplace {
            marketplace_url: Some("https://market.example.com".to_string()),
            plugin_id: "p1".to_string(),
        };
        let without_url = PluginSource::Marketplace {
            marketplace_url: None,
            plugin_id: "p2".to_string(),
        };
        assert!(
            matches!(with_url, PluginSource::Marketplace { marketplace_url: Some(_), plugin_id } if plugin_id == "p1")
        );
        assert!(
            matches!(without_url, PluginSource::Marketplace { marketplace_url: None, plugin_id } if plugin_id == "p2")
        );
    }

    #[test]
    fn test_plugin_source_storage_with_checksum() {
        let src = PluginSource::Storage {
            file_id: "fid".to_string(),
            checksum: Some("abc".to_string()),
        };
        match src {
            PluginSource::Storage { file_id, checksum } => {
                assert_eq!(file_id, "fid");
                assert_eq!(checksum.as_deref(), Some("abc"));
            }
            _ => panic!("应匹配 Storage 变体"),
        }
    }

    // ==================== app_id 默认值 ====================

    #[test]
    fn test_plugin_info_app_id_default_value() {
        // 通过反序列化验证 app_id 字段的默认值
        let json = r#"{
            "id": "p1",
            "name": "P1",
            "version": "1.0.0",
            "description": null,
            "author": null,
            "source": { "Local": { "path": "/tmp" } },
            "status": "installed",
            "installed_at": null,
            "updated_at": null,
            "install_path": "/tmp/p1",
            "plugin_type": "wasm",
            "source_path": null,
            "domain_code": "",
            "application_code": "",
            "module_code": ""
        }"#;
        let info: PluginInfo = serde_json::from_str(json).unwrap();
        assert_eq!(
            info.app_id, "default",
            "缺失 app_id 时应使用默认值 'default'"
        );
    }

    #[test]
    fn test_plugin_info_app_id_explicit_value() {
        let json = r#"{
            "id": "p1",
            "name": "P1",
            "version": "1.0.0",
            "description": null,
            "author": null,
            "source": { "Local": { "path": "/tmp" } },
            "status": "installed",
            "installed_at": null,
            "updated_at": null,
            "install_path": "/tmp/p1",
            "plugin_type": "wasm",
            "source_path": null,
            "domain_code": "",
            "application_code": "",
            "module_code": "",
            "app_id": "my-app"
        }"#;
        let info: PluginInfo = serde_json::from_str(json).unwrap();
        assert_eq!(info.app_id, "my-app");
    }
}
