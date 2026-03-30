//! 记录构建器模块
//!
//! 提供插件相关数据库记录的构建辅助函数

use std::path::Path;

use chrono::Utc;
use uuid::Uuid;

use crate::PluginDefinition;
use crate::infrastructure::database::deployment::DeploymentRecord;
use crate::infrastructure::database::repository::PluginDbRecord;
use crate::infrastructure::database::version_history::VersionHistoryRecord;

/// 构建插件数据库记录
///
/// # 参数
/// - `plugin_def`: 插件定义
/// - `version`: 插件版本
/// - `install_path`: 安装路径
/// - `db_id`: 数据库ID
/// - `zip_source_url`: 插件ZIP包来源地址
/// - `zip_source_type`: 插件来源类型
///
/// # 返回
/// 插件数据库记录
pub fn build_plugin_db_record(
    plugin_def: &PluginDefinition,
    version: &str,
    install_path: &Path,
    db_id: &str,
    zip_source_url: Option<&str>,
    zip_source_type: Option<&str>,
) -> PluginDbRecord {
    PluginDbRecord {
        id: Uuid::new_v4().to_string(),
        plugin_id: plugin_def.id.clone(),
        name: plugin_def.name.clone(),
        version: version.to_string(),
        wasm_path: install_path
            .join(&plugin_def.main_file)
            .to_string_lossy()
            .to_string(),
        install_path: install_path.to_string_lossy().to_string(),
        db_id: db_id.to_string(),
        status: "installed".to_string(),
        is_system: false,
        is_locked: false,
        domain_code: plugin_def.domain_code.clone(),
        application_code: plugin_def.application_code.clone(),
        module_code: plugin_def.module_code.clone(),
        vendor_name: plugin_def.vendor_name.clone(),
        vendor_url: plugin_def.vendor_url.clone(),
        vendor_contact: plugin_def.vendor_contact.clone(),
        metadata: None,
        signature_algorithm: None,
        signer_key_id: None,
        zip_source_url: zip_source_url.map(|s| s.to_string()),
        zip_source_type: zip_source_type.map(|s| s.to_string()),
        plugin_type: Some(plugin_def.r#type.clone()),
        source_path: plugin_def.source_path.clone(),
        create_time: Utc::now(),
        update_time: Utc::now(),
        archived: 0,
        create_by: None,
        create_name: None,
        update_by: None,
        update_name: None,
    }
}

/// 构建版本历史记录
///
/// # 参数
/// - `plugin_id`: 插件ID
/// - `version`: 版本号
/// - `install_path`: 安装路径
/// - `wasm_path`: WASM文件路径
/// - `zip_source_url`: 插件ZIP包来源地址
/// - `zip_source_type`: 插件来源类型
/// - `plugin_def`: 插件定义（用于获取 plugin_type 和 source_path）
///
/// # 返回
/// 版本历史记录
pub fn build_version_record(
    plugin_id: &str,
    version: &str,
    install_path: &str,
    wasm_path: &str,
    zip_source_url: Option<&str>,
    zip_source_type: Option<&str>,
    plugin_def: Option<&crate::PluginDefinition>,
) -> VersionHistoryRecord {
    let plugin_type = plugin_def.map(|d| d.r#type.clone());
    let source_path = plugin_def.and_then(|d| d.source_path.clone());
    VersionHistoryRecord {
        id: Uuid::new_v4().to_string(),
        plugin_id: plugin_id.to_string(),
        version: version.to_string(),
        install_path: install_path.to_string(),
        wasm_path: wasm_path.to_string(),
        is_current: true,
        installed_at: Utc::now(),
        uninstalled_at: None,
        zip_source_url: zip_source_url.map(|s| s.to_string()),
        zip_source_type: zip_source_type.map(|s| s.to_string()),
        plugin_type,
        source_path,
        create_time: Utc::now(),
        update_time: Utc::now(),
        archived: 0,
        create_by: None,
        create_name: None,
        update_by: None,
        update_name: None,
    }
}

/// 构建部署记录
///
/// # 参数
/// - `plugin_id`: 插件ID
/// - `node_id`: 节点ID
/// - `node_type`: 节点类型
/// - `version`: 版本号
///
/// # 返回
/// 部署记录
pub fn build_deployment_record(
    plugin_id: &str,
    node_id: &str,
    node_type: Option<&str>,
    version: &str,
) -> DeploymentRecord {
    DeploymentRecord {
        id: Uuid::new_v4().to_string(),
        plugin_id: plugin_id.to_string(),
        node_id: node_id.to_string(),
        node_type: node_type.map(|s| s.to_string()),
        version: version.to_string(),
        status: "deployed".to_string(),
        progress: 100,
        error_message: None,
        error_details: None,
        archived: 0,
        create_by: None,
        create_name: None,
        update_by: None,
        update_name: None,
        create_time: Utc::now(),
        update_time: Utc::now(),
    }
}

/// 从插件定义和安装路径构建 WASM 路径
///
/// # 参数
/// - `install_path`: 安装路径
/// - `plugin_def`: 插件定义
///
/// # 返回
/// WASM 文件的完整路径
pub fn build_wasm_path(install_path: &Path, plugin_def: &PluginDefinition) -> String {
    install_path
        .join(&plugin_def.main_file)
        .to_string_lossy()
        .to_string()
}
