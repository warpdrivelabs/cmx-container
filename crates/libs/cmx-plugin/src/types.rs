//! 插件生命周期管理类型定义

use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 插件依赖定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginDependency {
    pub plugin_id: String,
    pub version_constraint: String,
    pub is_optional: bool,
}

/// 插件扩展定义（包含依赖信息）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginExtendedDefinition {
    pub id: String,
    pub name: String,
    pub version: Option<String>,
    pub wasm_file: String,
    pub table_config_files: Vec<String>,
    pub supported_databases: Vec<String>,
    pub domain_code: Option<String>,
    pub application_code: Option<String>,
    pub module_code: Option<String>,
    pub vendor_name: Option<String>,
    pub vendor_url: Option<String>,
    pub vendor_contact: Option<String>,
    pub development_languages: Vec<String>,
    pub description: Option<String>,
    #[serde(default)]
    pub dependencies: Vec<PluginDependency>,
}

/// 部署策略
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum DeploymentStrategy {
    Serial { continue_on_error: bool },
    Parallel { max_concurrent: usize },
    Rolling { batch_size: usize, wait_seconds: u64 },
    BlueGreen { switch_at: Option<String> },
}

/// 插件数据库配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginDatabaseConfig {
    pub db_id: String,
    pub db_type: String,
    pub table_prefix: Option<String>,
    pub create_tables: bool,
}

/// 部署请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployRequest {
    pub plugin_id: String,
    pub version: String,
    pub nodes: Vec<String>,
    pub strategy: DeploymentStrategy,
    pub timeout: Option<u64>,
}

/// 插件状态枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PluginStatus {
    Installed,
    Active,
    Inactive,
    Failed,
    Uninstalling,
    Activating,
}

impl std::fmt::Display for PluginStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PluginStatus::Installed => write!(f, "installed"),
            PluginStatus::Active => write!(f, "active"),
            PluginStatus::Inactive => write!(f, "inactive"),
            PluginStatus::Failed => write!(f, "failed"),
            PluginStatus::Uninstalling => write!(f, "uninstalling"),
            PluginStatus::Activating => write!(f, "activating"),
        }
    }
}

/// 插件来源类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum PluginSource {
    Zip { path: String },
    Url {
        url: String,
        #[serde(default)]
        headers: HashMap<String, String>,
    },
    Registry {
        plugin_id: String,
        version: Option<String>,
    },
    Directory { path: String },
}

/// 安装请求结构体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallRequest {
    pub plugin_id: Option<String>,
    pub source: PluginSource,
    #[serde(default)]
    pub target_db_id: Option<String>,
    #[serde(default)]
    pub target_db_type: Option<String>,
    #[serde(default)]
    pub target_nodes: Option<Vec<String>>,
    #[serde(default)]
    pub config: Option<serde_json::Value>,
    #[serde(default)]
    pub force: bool,
    #[serde(default)]
    pub skip_validation: bool,
    pub operator: String,
}

/// 安装响应结构体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallResponse {
    pub success: bool,
    pub plugin_id: String,
    pub version: String,
    pub operation_id: String,
    #[serde(default)]
    pub nodes: Vec<NodeDeploymentResult>,
    pub duration_ms: u64,
}

/// 卸载请求结构体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UninstallRequest {
    pub plugin_id: String,
    #[serde(default)]
    pub force: bool,
    pub operator: String,
}

/// 卸载响应结构体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UninstallResponse {
    pub success: bool,
    pub operation_id: String,
    pub duration_ms: u64,
}

/// 激活请求结构体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivateRequest {
    pub plugin_id: String,
    pub operator: String,
}

/// 激活响应结构体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivateResponse {
    pub success: bool,
    pub plugin_id: String,
    pub operation_id: String,
    pub duration_ms: u64,
}

/// 停用请求结构体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeactivateRequest {
    pub plugin_id: String,
    pub operator: String,
}

/// 停用响应结构体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeactivateResponse {
    pub success: bool,
    pub plugin_id: String,
    pub operation_id: String,
    pub duration_ms: u64,
}

/// 升级请求结构体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpgradeRequest {
    pub plugin_id: String,
    pub source: PluginSource,
    pub operator: String,
}

/// 升级响应结构体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpgradeResponse {
    pub success: bool,
    pub plugin_id: String,
    pub from_version: String,
    pub to_version: String,
    pub operation_id: String,
    pub duration_ms: u64,
}

/// 降级请求结构体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DowngradeRequest {
    pub plugin_id: String,
    pub target_version: String,
    pub operator: String,
}

/// 降级响应结构体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DowngradeResponse {
    pub success: bool,
    pub plugin_id: String,
    pub from_version: String,
    pub to_version: String,
    pub operation_id: String,
    pub duration_ms: u64,
}

/// 回滚请求结构体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackRequest {
    pub operation_id: String,
    pub operator: String,
}

/// 回滚响应结构体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackResponse {
    pub success: bool,
    pub plugin_id: String,
    pub from_version: String,
    pub to_version: String,
    pub operation_id: String,
    pub duration_ms: u64,
}

/// 节点部署结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeDeploymentResult {
    pub node_id: String,
    pub success: bool,
    #[serde(default)]
    pub error_message: Option<String>,
}

/// 插件信息结构体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfo {
    pub plugin_id: String,
    pub name: String,
    pub version: String,
    pub status: PluginStatus,
    pub db_id: String,
    pub is_system: bool,
    pub wasm_path: String,
    pub install_path: String,
    #[serde(default)]
    pub domain_code: Option<String>,
    #[serde(default)]
    pub application_code: Option<String>,
    #[serde(default)]
    pub module_code: Option<String>,
    #[serde(default)]
    pub vendor_name: Option<String>,
    #[serde(default)]
    pub activated_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
}

/// 插件过滤器
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginFilter {
    #[serde(default)]
    pub status: Option<PluginStatus>,
    #[serde(default)]
    pub is_system: Option<bool>,
    #[serde(default)]
    pub domain_code: Option<String>,
    #[serde(default)]
    pub application_code: Option<String>,
    #[serde(default)]
    pub module_code: Option<String>,
}

/// 系统插件配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemPluginConfig {
    pub plugin_id: String,
    pub version: String,
    #[serde(default)]
    pub fallback_version: Option<String>,
    pub install_order: i32,
    #[serde(default)]
    pub is_optional: bool,
    #[serde(default)]
    pub is_critical: bool,
    pub retry_count: u32,
    pub source: PluginSource,
    #[serde(default)]
    pub metadata_db_id: Option<String>,
}

/// 插件管理器配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManagerConfig {
    pub install_root: PathBuf,
    pub temp_root: PathBuf,
    pub backup_root: PathBuf,
    pub max_concurrent_installs: usize,
    pub install_timeout_seconds: u64,
    pub upgrade_timeout_seconds: u64,
    pub require_signature: bool,
    #[serde(default)]
    pub trusted_signing_keys: Vec<String>,
    #[serde(default)]
    pub default_plugins: Vec<SystemPluginConfig>,
    pub default_db_id: String,
}

impl Default for PluginManagerConfig {
    fn default() -> Self {
        Self {
            install_root: PathBuf::from("plugins/"),
            temp_root: PathBuf::from("tmp/plugins/"),
            backup_root: PathBuf::from("backups/plugins/"),
            max_concurrent_installs: 4,
            install_timeout_seconds: 300,
            upgrade_timeout_seconds: 600,
            require_signature: false,
            trusted_signing_keys: Vec::new(),
            default_plugins: Vec::new(),
            default_db_id: "default".to_string(),
        }
    }
}

/// 版本类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VersionType {
    Initial,
    Upgrade,
    Downgrade,
    Rollback,
}

/// 版本关系
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VersionRelation {
    Greater,
    Equal,
    Less,
    Incompatible,
}

/// 兼容性级别
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompatibilityLevel {
    FullyCompatible,
    BackwardCompatible,
    ConditionallyCompatible,
    Incompatible,
    Unknown,
}

/// 依赖解析状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResolutionStatus {
    Resolved,
    Conflict,
    Missing,
    Pending,
}

/// 部署状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeploymentStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    RollingBack,
    RolledBack,
}

/// 操作类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OperationType {
    Install,
    Uninstall,
    Activate,
    Deactivate,
    Upgrade,
    Downgrade,
    Rollback,
    Deploy,
    Sync,
    Recovery,
    ConfigUpdate,
    SignatureVerify,
    DependencyResolve,
}

impl OperationType {
    /// 转换为字符串
    pub fn as_str(&self) -> &'static str {
        match self {
            OperationType::Install => "install",
            OperationType::Uninstall => "uninstall",
            OperationType::Activate => "activate",
            OperationType::Deactivate => "deactivate",
            OperationType::Upgrade => "upgrade",
            OperationType::Downgrade => "downgrade",
            OperationType::Rollback => "rollback",
            OperationType::Deploy => "deploy",
            OperationType::Sync => "sync",
            OperationType::Recovery => "recovery",
            OperationType::ConfigUpdate => "config_update",
            OperationType::SignatureVerify => "signature_verify",
            OperationType::DependencyResolve => "dependency_resolve",
        }
    }
}

/// 操作状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OperationStatus {
    Pending,
    InProgress,
    Success,
    Failed,
    PartialFailed,
}

impl OperationStatus {
    /// 转换为字符串
    pub fn as_str(&self) -> &'static str {
        match self {
            OperationStatus::Pending => "pending",
            OperationStatus::InProgress => "in_progress",
            OperationStatus::Success => "success",
            OperationStatus::Failed => "failed",
            OperationStatus::PartialFailed => "partial_failed",
        }
    }
}

/// 升级路径
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpgradePath {
    pub from: String,
    pub to: String,
    pub steps: Vec<UpgradeStep>,
    pub is_safe: bool,
    pub warnings: Vec<String>,
}

/// 升级步骤
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpgradeStep {
    pub version: String,
    pub description: String,
}

/// 依赖解析结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyResolution {
    pub resolved: HashMap<String, String>,
    pub conflicts: Vec<DependencyConflict>,
    pub missing: Vec<MissingDependency>,
}

/// 依赖冲突
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyConflict {
    pub plugin_id: String,
    pub required_version: String,
    pub existing_version: String,
}

/// 缺失依赖
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissingDependency {
    pub plugin_id: String,
    pub constraint: String,
}

/// 兼容性检查结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompatibilityResult {
    pub level: CompatibilityLevel,
    pub breaking_changes: Vec<BreakingChange>,
    pub warnings: Vec<String>,
    #[serde(default)]
    pub migration_guide: Option<String>,
}

/// 破坏性变更
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BreakingChange {
    pub category: String,
    pub description: String,
    pub severity: String,
    #[serde(default)]
    pub migration: Option<String>,
}

/// 依赖检查结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyCheckResult {
    pub satisfied: bool,
    pub missing: Vec<MissingDependency>,
    pub conflicts: Vec<DependencyConflict>,
}

/// 依赖图节点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepNode {
    pub plugin_id: String,
    pub version: String,
    pub is_root: bool,
}

/// 依赖图边
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepEdge {
    pub from: String,
    pub to: String,
    pub constraint: String,
}

/// 依赖图
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DependencyGraph {
    pub nodes: Vec<DepNode>,
    pub edges: Vec<DepEdge>,
}

/// 默认插件配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultPluginsConfig {
    #[serde(default)]
    pub settings: SettingsConfig,
    #[serde(default)]
    pub required: Vec<PluginConfig>,
    #[serde(default)]
    pub optional: Vec<PluginConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SettingsConfig {
    #[serde(default = "default_install_root")]
    pub install_root: String,
    #[serde(default = "default_temp_dir")]
    pub temp_dir: String,
    #[serde(default = "default_db_id")]
    pub default_db_id: String,
}

fn default_install_root() -> String {
    "plugins/".to_string()
}

fn default_temp_dir() -> String {
    "tmp/plugins/".to_string()
}

fn default_db_id() -> String {
    "default".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginConfig {
    pub id: String,
    pub version: String,
    pub source: String,
    #[serde(default)]
    pub metadata_db_id: Option<String>,
}

/// 初始化结果
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InitResult {
    pub required_succeeded: usize,
    pub required_failed: usize,
    pub optional_succeeded: usize,
    pub optional_failed: usize,
    #[serde(default)]
    pub critical_errors: Vec<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
}
