//! 插件上下文模块
//!
//! 管理插件运行时状态

use std::collections::HashMap;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};

use crate::domain::plugin::PluginStatus;

/// 插件上下文 - 管理插件运行时状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginContext {
    /// 插件ID
    pub plugin_id: String,
    /// 插件版本
    pub version: String,
    /// 插件状态
    pub status: PluginStatus,
    /// 关联的数据库ID
    pub db_id: String,
    /// 安装路径
    pub install_path: PathBuf,
    /// WASM路径
    pub wasm_path: PathBuf,
    /// 插件类型
    pub plugin_type: Option<String>,
    /// 源码路径
    pub source_path: Option<String>,
    /// 服务句柄列表
    pub services: Vec<String>,
    /// 扩展元数据
    pub metadata: HashMap<String, serde_json::Value>,
}

impl PluginContext {
    /// 创建新的插件上下文
    pub fn new(plugin_id: String, version: String) -> Self {
        Self {
            plugin_id,
            version,
            status: PluginStatus::Installed,
            db_id: String::new(),
            install_path: PathBuf::new(),
            wasm_path: PathBuf::new(),
            plugin_type: None,
            source_path: None,
            services: Vec::new(),
            metadata: HashMap::new(),
        }
    }

    /// 从插件定义创建上下文
    pub fn from_definition(def: &cmx_core::model::meta::plugin::PluginDefinition, install_path: &Path) -> Self {
        Self {
            plugin_id: def.id.clone(),
            version: def.version.clone().unwrap_or_else(|| "1.0.0".to_string()),
            status: PluginStatus::Installed,
            db_id: String::new(),
            install_path: install_path.to_path_buf(),
            wasm_path: install_path.join(&def.main_file),
            plugin_type: Some(def.r#type.clone()),
            source_path: def.source_path.clone(),
            services: Vec::new(),
            metadata: HashMap::new(),
        }
    }

    /// 从数据库记录创建上下文
    pub fn from_db_record(record: &crate::infrastructure::database::plugin::PluginRecord) -> Self {
        Self {
            plugin_id: record.plugin_id.clone(),
            version: record.version.clone(),
            status: match record.status.as_str() {
                "installed" => PluginStatus::Installed,
                "activated" => PluginStatus::Activated,
                "deactivated" => PluginStatus::Deactivated,
                "error" => PluginStatus::Error,
                _ => PluginStatus::Installed,
            },
            db_id: record.db_id.clone(),
            install_path: PathBuf::from(&record.install_path),
            wasm_path: PathBuf::from(&record.wasm_path),
            plugin_type: record.plugin_type.clone(),
            source_path: record.source_path.clone(),
            services: Vec::new(),
            metadata: HashMap::new(),
        }
    }

    // /// 转换为数据库记录
    // pub fn to_db_record(&self) -> crate::infrastructure::database::repository::PluginDbRecord {
    //     crate::infrastructure::database::repository::PluginDbRecord {
    //         id: uuid::Uuid::new_v4().to_string(),
    //         plugin_id: self.plugin_id.clone(),
    //         name: self.plugin_id.clone(),
    //         version: self.version.clone(),
    //         wasm_path: self.wasm_path.to_string_lossy().to_string(),
    //         install_path: self.install_path.to_string_lossy().to_string(),
    //         db_id: self.db_id.clone(),
    //         status: self.status.to_string(),
    //         is_system: false,
    //         is_locked: false,
    //         domain_code: None,
    //         application_code: None,
    //         module_code: None,
    //         vendor_name: None,
    //         vendor_url: None,
    //         vendor_contact: None,
    //         metadata: self.config.clone(),
    //         signature_algorithm: None,
    //         signer_key_id: None,
    //         create_time: Utc::now(),
    //         update_time: Utc::now(),
    //         archived: 0,
    //         create_by: None,
    //         create_name: None,
    //         update_by: None,
    //         update_name: None,
    //     }
    // }
}

use std::path::Path;
