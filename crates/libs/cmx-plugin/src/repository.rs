//! 数据库仓库模块 - 插件数据持久化
//!
//! 封装插件相关的数据库操作，包括插件表、版本表、依赖表、部署记录表、审计日志表等。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 插件数据库仓库
pub struct PluginRepository {
    db_id: String,
}

impl PluginRepository {
    /// 创建新的插件仓库
    pub fn new(db_id: impl Into<String>) -> Self {
        Self {
            db_id: db_id.into(),
        }
    }

    /// 获取数据库 ID
    pub fn db_id(&self) -> &str {
        &self.db_id
    }
}

/// 插件记录（数据库映射）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginRecord {
    pub id: i64,
    pub plugin_id: String,
    pub name: String,
    pub version: String,
    pub status: String,
    pub wasm_path: String,
    pub install_path: String,
    pub config_path: Option<String>,
    pub db_id: String,
    pub is_system: bool,
    pub is_locked: bool,
    pub domain_code: Option<String>,
    pub application_code: Option<String>,
    pub module_code: Option<String>,
    pub vendor_name: Option<String>,
    pub vendor_url: Option<String>,
    pub vendor_contact: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub signature_algorithm: Option<String>,
    pub signer_key_id: Option<String>,
    pub activated_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 版本历史记录（数据库映射）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionRecord {
    pub id: i64,
    pub plugin_id: i64,
    pub version: String,
    pub version_type: String,
    pub from_version: Option<String>,
    pub install_path: String,
    pub wasm_path: String,
    pub backup_path: Option<String>,
    pub is_current: bool,
    pub installed_at: DateTime<Utc>,
    pub uninstalled_at: Option<DateTime<Utc>>,
    pub installed_by: Option<String>,
    pub install_reason: Option<String>,
}

/// 依赖关系记录（数据库映射）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyRecord {
    pub id: i64,
    pub plugin_id: i64,
    pub dependency_plugin_id: String,
    pub version_constraint: String,
    pub is_optional: bool,
    pub resolved_version: Option<String>,
}

/// 部署记录（数据库映射）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentRecord {
    pub id: i64,
    pub plugin_id: i64,
    pub node_id: String,
    pub version: String,
    pub status: String,
    pub deployed_at: DateTime<Utc>,
    pub error_message: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// 审计日志记录（数据库映射）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditRecord {
    pub id: i64,
    pub plugin_id: i64,
    pub operation_type: String,
    pub operator: String,
    pub status: String,
    pub details: serde_json::Value,
    pub error_message: Option<String>,
    pub client_ip: Option<String>,
    pub user_agent: Option<String>,
    pub timestamp: DateTime<Utc>,
}

/// 回滚记录（数据库映射）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackRecord {
    pub id: i64,
    pub operation_id: String,
    pub plugin_id: i64,
    pub from_version: String,
    pub to_version: String,
    pub backup_path: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

/// 系统默认插件记录（数据库映射）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemPluginRecord {
    pub id: i64,
    pub plugin_id: String,
    pub version: String,
    pub fallback_version: Option<String>,
    pub install_order: i32,
    pub is_optional: bool,
    pub retry_count: i32,
    pub source_type: String,
}

/// 节点记录（数据库映射）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeRecord {
    pub id: i64,
    pub node_id: String,
    pub node_name: String,
    pub node_type: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

/// 插件文件记录（数据库映射）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginFileRecord {
    pub id: i64,
    pub plugin_id: i64,
    pub file_path: String,
    pub file_type: String,
    pub file_hash: String,
    pub file_size: i64,
    pub created_at: DateTime<Utc>,
}
