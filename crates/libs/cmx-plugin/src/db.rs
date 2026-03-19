//! 数据库访问模块 - 插件数据持久化（抽象接口）
//!
//! 定义插件数据库操作的 trait 接口，具体实现由使用方注入。
//! 支持对接 cmx-database 或其他数据库实现。

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 插件数据库 trait - 定义插件数据持久化操作
///
/// 使用方可以注入 cmx-database 或其他数据库实现
#[async_trait]
pub trait PluginDatabase: Send + Sync {
    /// 插入插件记录
    async fn insert_plugin(&self, record: &PluginDbRecord) -> Result<(), PluginDbError>;

    /// 更新插件记录
    async fn update_plugin(&self, db_id: &str, plugin_id: &str, updates: &PluginUpdateFields) -> Result<(), PluginDbError>;

    /// 删除插件记录
    async fn delete_plugin(&self, db_id: &str, plugin_id: &str) -> Result<(), PluginDbError>;

    /// 根据 plugin_id 查询插件
    async fn get_plugin_by_id(&self, db_id: &str, plugin_id: &str) -> Result<Option<PluginDbRecord>, PluginDbError>;

    /// 查询所有插件
    async fn get_all_plugins(&self, db_id: &str) -> Result<Vec<PluginDbRecord>, PluginDbError>;

    /// 插入版本记录
    async fn insert_version(&self, db_id: &str, record: &VersionDbRecord) -> Result<(), PluginDbError>;

    /// 插入审计日志记录
    async fn insert_audit_log(&self, db_id: &str, record: &AuditDbRecord) -> Result<(), PluginDbError>;

    /// 查询审计日志
    async fn query_audit_logs(&self, db_id: &str, plugin_id: Option<&str>, operation_type: Option<&str>, limit: u64) -> Result<Vec<AuditDbRecord>, PluginDbError>;

    /// 插入部署记录
    async fn insert_deployment(&self, db_id: &str, record: &DeploymentDbRecord) -> Result<(), PluginDbError>;

    /// 插入回滚记录
    async fn insert_rollback(&self, db_id: &str, record: &RollbackDbRecord) -> Result<(), PluginDbError>;
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
    pub create_time: DateTime<Utc>,
    pub update_time: DateTime<Utc>,
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
