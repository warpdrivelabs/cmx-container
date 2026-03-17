//! 数据库访问模块 - 插件数据持久化（抽象接口）
//!
//! 定义插件数据库操作的 trait 接口，具体实现由使用方注入。
//! 支持对接 cmx-database 或其他数据库实现。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 插件数据库 trait - 定义插件数据持久化操作
/// 
/// 使用方可以注入 cmx-database 或其他数据库实现
pub trait PluginDatabase: Send + Sync {
    /// 插入插件记录
    fn insert_plugin(&self, record: &PluginDbRecord) -> impl std::future::Future<Output = Result<(), PluginDbError>> + Send;

    /// 更新插件记录
    fn update_plugin(&self, db_id: &str, plugin_id: &str, updates: &PluginUpdateFields) -> impl std::future::Future<Output = Result<(), PluginDbError>> + Send;

    /// 删除插件记录
    fn delete_plugin(&self, db_id: &str, plugin_id: &str) -> impl std::future::Future<Output = Result<(), PluginDbError>> + Send;

    /// 根据 plugin_id 查询插件
    fn get_plugin_by_id(&self, db_id: &str, plugin_id: &str) -> impl std::future::Future<Output = Result<Option<PluginDbRecord>, PluginDbError>> + Send;

    /// 查询所有插件
    fn get_all_plugins(&self, db_id: &str) -> impl std::future::Future<Output = Result<Vec<PluginDbRecord>, PluginDbError>> + Send;

    /// 插入版本记录
    fn insert_version(&self, db_id: &str, record: &VersionDbRecord) -> impl std::future::Future<Output = Result<(), PluginDbError>> + Send;

    /// 插入审计日志记录
    fn insert_audit_log(&self, db_id: &str, record: &AuditDbRecord) -> impl std::future::Future<Output = Result<(), PluginDbError>> + Send;

    /// 查询审计日志
    fn query_audit_logs(&self, db_id: &str, plugin_id: Option<&str>, operation_type: Option<&str>, limit: u64) -> impl std::future::Future<Output = Result<Vec<AuditDbRecord>, PluginDbError>> + Send;

    /// 插入部署记录
    fn insert_deployment(&self, db_id: &str, record: &DeploymentDbRecord) -> impl std::future::Future<Output = Result<(), PluginDbError>> + Send;

    /// 插入回滚记录
    fn insert_rollback(&self, db_id: &str, record: &RollbackDbRecord) -> impl std::future::Future<Output = Result<(), PluginDbError>> + Send;
}

/// 插件数据库服务 - 封装插件相关的数据库操作
pub struct PluginDbService;

impl PluginDbService {
    /// 创建新的插件数据库服务
    pub fn new() -> Self {
        Self
    }
}

impl Default for PluginDbService {
    fn default() -> Self {
        Self::new()
    }
}

/// 插件数据库记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginDbRecord {
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

/// 插件更新字段
#[derive(Debug, Clone, Default)]
pub struct PluginUpdateFields {
    pub version: Option<String>,
    pub status: Option<String>,
    pub wasm_path: Option<String>,
    pub install_path: Option<String>,
    pub activated_at: Option<DateTime<Utc>>,
}

/// 版本数据库记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionDbRecord {
    pub plugin_id: String,
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

/// 审计日志数据库记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditDbRecord {
    pub plugin_id: String,
    pub operation_type: String,
    pub operator: String,
    pub status: String,
    pub details: serde_json::Value,
    pub error_message: Option<String>,
    pub client_ip: Option<String>,
    pub user_agent: Option<String>,
    pub timestamp: DateTime<Utc>,
}

/// 部署记录数据库结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentDbRecord {
    pub plugin_id: String,
    pub node_id: String,
    pub version: String,
    pub status: String,
    pub deployed_at: DateTime<Utc>,
    pub error_message: Option<String>,
}

/// 回滚记录数据库结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackDbRecord {
    pub operation_id: String,
    pub plugin_id: String,
    pub from_version: String,
    pub to_version: String,
    pub backup_path: String,
    pub status: String,
}

/// 插件数据库错误
#[derive(Debug, thiserror::Error)]
pub enum PluginDbError {
    #[error("数据库操作错误: {0}")]
    Operation(String),
    #[error("数据库查询错误: {0}")]
    Query(String),
    #[error("数据库连接错误: {0}")]
    Connection(String),
    #[error("记录不存在: {0}")]
    NotFound(String),
}
