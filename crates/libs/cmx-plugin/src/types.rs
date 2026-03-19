//! 插件生命周期管理类型定义模块
//!
//! 定义插件系统中使用的各种数据结构、枚举类型和请求/响应结构体，
//! 涵盖插件的安装、卸载、激活、停用、升级、降级、回滚等生命周期操作。

use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 插件依赖定义
///
/// 描述一个插件对其他插件的依赖关系，包括依赖的插件ID、版本约束和是否为可选依赖。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginDependency {
    /// 被依赖的插件唯一标识符
    pub plugin_id: String,
    /// 版本约束表达式（如 "^1.0.0"、"~2.1.0" 或 ">=1.0.0 <3.0.0"）
    pub version_constraint: String,
    /// 是否为可选依赖，可选依赖在未满足时不会阻止插件安装
    pub is_optional: bool,
}

/// 插件扩展定义（包含依赖信息）
///
/// 完整的插件定义结构，包含插件的所有元数据信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginExtendedDefinition {
    /// 插件唯一标识符
    pub id: String,
    /// 插件显示名称
    pub name: String,
    /// 插件版本号（语义化版本）
    pub version: Option<String>,
    /// WASM 文件路径（相对于插件根目录）
    pub wasm_file: String,
    /// 表结构配置文件路径列表
    pub table_config_files: Vec<String>,
    /// 支持的数据库类型列表
    pub supported_databases: Vec<String>,
    /// 所属领域代码
    pub domain_code: Option<String>,
    /// 所属应用代码
    pub application_code: Option<String>,
    /// 所属模块代码
    pub module_code: Option<String>,
    /// 插件供应商名称
    pub vendor_name: Option<String>,
    /// 插件供应商 URL
    pub vendor_url: Option<String>,
    /// 插件供应商联系方式
    pub vendor_contact: Option<String>,
    /// 支持的开发语言列表
    pub development_languages: Vec<String>,
    /// 插件描述信息
    pub description: Option<String>,
    /// 插件依赖列表
    #[serde(default)]
    pub dependencies: Vec<PluginDependency>,
}

/// 部署策略枚举
///
/// 定义插件部署到多个节点时可以采用的策略。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum DeploymentStrategy {
    /// 串行部署：按顺序逐个部署到节点
    Serial { continue_on_error: bool },
    /// 并行部署：同时部署到多个节点
    Parallel { max_concurrent: usize },
    /// 滚动部署：分批部署，每批之间等待
    Rolling { batch_size: usize, wait_seconds: u64 },
    /// 蓝绿部署：同时部署新旧版本，切换时只需切换流量
    BlueGreen { switch_at: Option<String> },
}

/// 插件数据库配置
///
/// 描述插件使用的数据库配置信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginDatabaseConfig {
    /// 数据库唯一标识符
    pub db_id: String,
    /// 数据库类型（如 "postgres"、"mysql" 等）
    pub db_type: String,
    /// 表名前缀（可选）
    pub table_prefix: Option<String>,
    /// 是否在安装时自动创建表结构
    pub create_tables: bool,
}

/// 部署请求结构体
///
/// 请求将插件部署到指定节点的参数。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployRequest {
    /// 插件唯一标识符
    pub plugin_id: String,
    /// 目标版本号
    pub version: String,
    /// 目标节点 ID 列表
    pub nodes: Vec<String>,
    /// 部署策略
    pub strategy: DeploymentStrategy,
    /// 部署超时时间（秒）
    pub timeout: Option<u64>,
}

/// 插件状态枚举
///
/// 描述插件当前所处的生命周期状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PluginStatus {
    /// 已安装：插件已成功安装但未激活
    Installed,
    /// 已激活：插件已激活并可以执行
    Active,
    /// 未激活：插件已安装但当前处于停用状态
    Inactive,
    /// 失败：插件上次操作失败
    Failed,
    /// 正在卸载：卸载操作进行中
    Uninstalling,
    /// 正在激活：激活操作进行中
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

/// 插件来源类型枚举
///
/// 描述插件包的来源方式，支持本地文件、远程URL、插件注册表和本地目录。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum PluginSource {
    /// ZIP 文件来源：本地 ZIP 压缩包路径
    Zip { path: String },
    /// URL 来源：远程下载链接
    Url {
        url: String,
        #[serde(default)]
        headers: HashMap<String, String>,
    },
    /// 注册表来源：从插件注册表查询并下载
    Registry {
        plugin_id: String,
        version: Option<String>,
    },
    /// 目录来源：本地解压后的目录路径
    Directory { path: String },
}

/// 安装请求结构体
///
/// 请求安装插件的参数，包含插件来源、目标数据库、部署节点等信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallRequest {
    /// 插件唯一标识符（可选，如果从 manifest.json 读取则不需要）
    pub plugin_id: Option<String>,
    /// 插件来源配置（ZIP文件路径、URL、注册表或目录）
    pub source: PluginSource,
    /// 目标数据库 ID（可选，默认使用配置的默认数据库）
    #[serde(default)]
    pub target_db_id: Option<String>,
    /// 目标数据库类型（可选）
    #[serde(default)]
    pub target_db_type: Option<String>,
    /// 目标节点列表（可选，用于多节点部署）
    #[serde(default)]
    pub target_nodes: Option<Vec<String>>,
    /// 插件配置（JSON 格式的可选配置参数）
    #[serde(default)]
    pub config: Option<serde_json::Value>,
    /// 是否强制安装（如果已存在则覆盖）
    #[serde(default)]
    pub force: bool,
    /// 是否跳过安全验证
    #[serde(default)]
    pub skip_validation: bool,
    /// 操作者标识（用于审计日志）
    pub operator: String,
}

/// 安装响应结构体
///
/// 插件安装操作的结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallResponse {
    /// 安装是否成功
    pub success: bool,
    /// 插件唯一标识符
    pub plugin_id: String,
    /// 安装的版本号
    pub version: String,
    /// 操作唯一标识符（用于追踪和回滚）
    pub operation_id: String,
    /// 各节点部署结果
    #[serde(default)]
    pub nodes: Vec<NodeDeploymentResult>,
    /// 操作耗时（毫秒）
    pub duration_ms: u64,
}

/// 卸载请求结构体
///
/// 请求卸载插件的参数。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UninstallRequest {
    /// 插件唯一标识符
    pub plugin_id: String,
    /// 是否强制卸载（即使有依赖关系）
    #[serde(default)]
    pub force: bool,
    /// 操作者标识（用于审计日志）
    pub operator: String,
}

/// 卸载响应结构体
///
/// 插件卸载操作的结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UninstallResponse {
    /// 卸载是否成功
    pub success: bool,
    /// 操作唯一标识符
    pub operation_id: String,
    /// 操作耗时（毫秒）
    pub duration_ms: u64,
}

/// 激活请求结构体
///
/// 请求激活已安装插件的参数。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivateRequest {
    /// 插件唯一标识符
    pub plugin_id: String,
    /// 操作者标识（用于审计日志）
    pub operator: String,
}

/// 激活响应结构体
///
/// 插件激活操作的结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivateResponse {
    /// 激活是否成功
    pub success: bool,
    /// 插件唯一标识符
    pub plugin_id: String,
    /// 操作唯一标识符
    pub operation_id: String,
    /// 操作耗时（毫秒）
    pub duration_ms: u64,
}

/// 停用请求结构体
///
/// 请求停用已激活插件的参数。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeactivateRequest {
    /// 插件唯一标识符
    pub plugin_id: String,
    /// 操作者标识（用于审计日志）
    pub operator: String,
}

/// 停用响应结构体
///
/// 插件停用操作的结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeactivateResponse {
    /// 停用是否成功
    pub success: bool,
    /// 插件唯一标识符
    pub plugin_id: String,
    /// 操作唯一标识符
    pub operation_id: String,
    /// 操作耗时（毫秒）
    pub duration_ms: u64,
}

/// 升级请求结构体
///
/// 请求升级插件到新版本的参数。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpgradeRequest {
    /// 插件唯一标识符
    pub plugin_id: String,
    /// 新版本来源
    pub source: PluginSource,
    /// 操作者标识（用于审计日志）
    pub operator: String,
}

/// 升级响应结构体
///
/// 插件升级操作的结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpgradeResponse {
    /// 升级是否成功
    pub success: bool,
    /// 插件唯一标识符
    pub plugin_id: String,
    /// 起始版本号
    pub from_version: String,
    /// 目标版本号
    pub to_version: String,
    /// 操作唯一标识符
    pub operation_id: String,
    /// 操作耗时（毫秒）
    pub duration_ms: u64,
}

/// 降级请求结构体
///
/// 请求降级插件到旧版本的参数。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DowngradeRequest {
    /// 插件唯一标识符
    pub plugin_id: String,
    /// 目标版本号
    pub target_version: String,
    /// 操作者标识（用于审计日志）
    pub operator: String,
}

/// 降级响应结构体
///
/// 插件降级操作的结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DowngradeResponse {
    /// 降级是否成功
    pub success: bool,
    /// 插件唯一标识符
    pub plugin_id: String,
    /// 起始版本号
    pub from_version: String,
    /// 目标版本号
    pub to_version: String,
    /// 操作唯一标识符
    pub operation_id: String,
    /// 操作耗时（毫秒）
    pub duration_ms: u64,
}

/// 回滚请求结构体
///
/// 请求回滚插件到之前版本的参数。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackRequest {
    /// 操作唯一标识符（指定要回滚的操作）
    pub operation_id: String,
    /// 操作者标识（用于审计日志）
    pub operator: String,
}

/// 回滚响应结构体
///
/// 插件回滚操作的结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackResponse {
    /// 回滚是否成功
    pub success: bool,
    /// 插件唯一标识符
    pub plugin_id: String,
    /// 回滚前的版本号
    pub from_version: String,
    /// 回滚后的版本号
    pub to_version: String,
    /// 操作唯一标识符
    pub operation_id: String,
    /// 操作耗时（毫秒）
    pub duration_ms: u64,
}

/// 节点部署结果
///
/// 描述插件在单个节点上的部署结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeDeploymentResult {
    /// 节点唯一标识符
    pub node_id: String,
    /// 部署是否成功
    pub success: bool,
    /// 错误信息（如果部署失败）
    #[serde(default)]
    pub error_message: Option<String>,
}

/// 插件信息结构体
///
/// 包含插件的完整信息，用于查询和展示。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfo {
    /// 插件唯一标识符
    pub plugin_id: String,
    /// 插件显示名称
    pub name: String,
    /// 当前版本号
    pub version: String,
    /// 插件当前状态
    pub status: PluginStatus,
    /// 关联的数据库 ID
    pub db_id: String,
    /// 是否为系统插件
    pub is_system: bool,
    /// WASM 文件路径
    pub wasm_path: String,
    /// 安装目录路径
    pub install_path: String,
    /// 领域代码
    #[serde(default)]
    pub domain_code: Option<String>,
    /// 应用代码
    #[serde(default)]
    pub application_code: Option<String>,
    /// 模块代码
    #[serde(default)]
    pub module_code: Option<String>,
    /// 供应商名称
    #[serde(default)]
    pub vendor_name: Option<String>,
    /// 激活时间
    #[serde(default)]
    pub activated_at: Option<DateTime<Utc>>,
    /// 创建时间
    #[serde(default)]
    pub create_time: Option<DateTime<Utc>>,
    /// 更新时间
    #[serde(default)]
    pub update_time: Option<DateTime<Utc>>,
}

/// 插件过滤器
///
/// 用于查询和筛选插件的条件。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginFilter {
    /// 按状态筛选
    #[serde(default)]
    pub status: Option<PluginStatus>,
    /// 按是否为系统插件筛选
    #[serde(default)]
    pub is_system: Option<bool>,
    /// 按领域代码筛选
    #[serde(default)]
    pub domain_code: Option<String>,
    /// 按应用代码筛选
    #[serde(default)]
    pub application_code: Option<String>,
    /// 按模块代码筛选
    #[serde(default)]
    pub module_code: Option<String>,
}

/// 系统插件配置
///
/// 定义系统启动时需要自动安装的插件。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemPluginConfig {
    /// 插件唯一标识符
    pub plugin_id: String,
    /// 插件版本约束
    pub version: String,
    /// 回退版本（当主版本安装失败时尝试）
    #[serde(default)]
    pub fallback_version: Option<String>,
    /// 安装顺序（数字越小越先安装）
    pub install_order: i32,
    /// 是否为可选插件
    #[serde(default)]
    pub is_optional: bool,
    /// 是否为关键插件（关键插件安装失败会阻止系统启动）
    #[serde(default)]
    pub is_critical: bool,
    /// 安装失败重试次数
    pub retry_count: u32,
    /// 插件来源配置
    pub source: PluginSource,
    /// 插件元数据数据库 ID
    #[serde(default)]
    pub metadata_db_id: Option<String>,
}

/// 插件管理器配置
///
/// 配置插件管理器的各种行为参数。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManagerConfig {
    /// 插件安装根目录
    pub install_root: PathBuf,
    /// 临时文件目录
    pub temp_root: PathBuf,
    /// 备份文件目录
    pub backup_root: PathBuf,
    /// 最大并发安装数
    pub max_concurrent_installs: usize,
    /// 安装超时时间（秒）
    pub install_timeout_seconds: u64,
    /// 升级超时时间（秒）
    pub upgrade_timeout_seconds: u64,
    /// 是否要求签名验证
    pub require_signature: bool,
    /// 受信任的签名密钥列表
    #[serde(default)]
    pub trusted_signing_keys: Vec<String>,
    /// 默认系统插件列表
    #[serde(default)]
    pub default_plugins: Vec<SystemPluginConfig>,
    /// 默认数据库 ID
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

/// 版本类型枚举
///
/// 描述版本变更的类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VersionType {
    /// 初始版本
    Initial,
    /// 升级版本
    Upgrade,
    /// 降级版本
    Downgrade,
    /// 回滚版本
    Rollback,
}

/// 版本关系枚举
///
/// 描述两个版本之间的比较关系。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VersionRelation {
    /// 大于
    Greater,
    /// 等于
    Equal,
    /// 小于
    Less,
    /// 不兼容（无法比较）
    Incompatible,
}

/// 兼容性级别枚举
///
/// 描述两个版本之间的兼容程度。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompatibilityLevel {
    /// 完全兼容
    FullyCompatible,
    /// 向后兼容
    BackwardCompatible,
    /// 有条件兼容
    ConditionallyCompatible,
    /// 不兼容
    Incompatible,
    /// 未知
    Unknown,
}

/// 依赖解析状态枚举
///
/// 描述依赖解析的结果状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResolutionStatus {
    /// 已解析
    Resolved,
    /// 存在冲突
    Conflict,
    /// 缺少依赖
    Missing,
    /// 待处理
    Pending,
}

/// 部署状态枚举
///
/// 描述部署操作的当前状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeploymentStatus {
    /// 待处理
    Pending,
    /// 进行中
    InProgress,
    /// 已完成
    Completed,
    /// 失败
    Failed,
    /// 正在回滚
    RollingBack,
    /// 已回滚
    RolledBack,
}

/// 操作类型枚举
///
/// 描述插件生命周期中的各种操作类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OperationType {
    /// 安装
    Install,
    /// 卸载
    Uninstall,
    /// 激活
    Activate,
    /// 停用
    Deactivate,
    /// 升级
    Upgrade,
    /// 降级
    Downgrade,
    /// 回滚
    Rollback,
    /// 部署
    Deploy,
    /// 同步
    Sync,
    /// 恢复
    Recovery,
    /// 配置更新
    ConfigUpdate,
    /// 签名验证
    SignatureVerify,
    /// 依赖解析
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

/// 操作状态枚举
///
/// 描述操作的执行状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OperationStatus {
    /// 待处理
    Pending,
    /// 进行中
    InProgress,
    /// 成功
    Success,
    /// 失败
    Failed,
    /// 部分失败
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

/// 升级路径结构体
///
/// 描述从一个版本升级到另一个版本的路径信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpgradePath {
    /// 起始版本
    pub from: String,
    /// 目标版本
    pub to: String,
    /// 升级步骤列表
    pub steps: Vec<UpgradeStep>,
    /// 是否为安全升级
    pub is_safe: bool,
    /// 警告信息列表
    pub warnings: Vec<String>,
}

/// 升级步骤结构体
///
/// 描述升级过程中的单个步骤。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpgradeStep {
    /// 目标版本
    pub version: String,
    /// 步骤描述
    pub description: String,
}

/// 依赖解析结果结构体
///
/// 描述依赖解析的完整结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyResolution {
    /// 已解析的依赖映射（插件ID -> 版本）
    pub resolved: HashMap<String, String>,
    /// 冲突的依赖列表
    pub conflicts: Vec<DependencyConflict>,
    /// 缺失的依赖列表
    pub missing: Vec<MissingDependency>,
}

/// 依赖冲突结构体
///
/// 描述两个版本之间的冲突。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyConflict {
    /// 插件唯一标识符
    pub plugin_id: String,
    /// 要求的版本
    pub required_version: String,
    /// 已存在的版本
    pub existing_version: String,
}

/// 缺失依赖结构体
///
/// 描述一个缺失的依赖。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissingDependency {
    /// 插件唯一标识符
    pub plugin_id: String,
    /// 版本约束
    pub constraint: String,
}

/// 兼容性检查结果结构体
///
/// 描述两个版本之间的兼容性检查结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompatibilityResult {
    /// 兼容性级别
    pub level: CompatibilityLevel,
    /// 破坏性变更列表
    pub breaking_changes: Vec<BreakingChange>,
    /// 警告信息列表
    pub warnings: Vec<String>,
    /// 迁移指南（可选）
    #[serde(default)]
    pub migration_guide: Option<String>,
}

/// 破坏性变更结构体
///
/// 描述一个破坏性变更。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BreakingChange {
    /// 变更类别
    pub category: String,
    /// 变更描述
    pub description: String,
    /// 严重程度
    pub severity: String,
    /// 迁移建议（可选）
    #[serde(default)]
    pub migration: Option<String>,
}

/// 依赖检查结果结构体
///
/// 描述插件依赖检查的结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyCheckResult {
    /// 是否满足所有依赖
    pub satisfied: bool,
    /// 缺失的依赖列表
    pub missing: Vec<MissingDependency>,
    /// 冲突的依赖列表
    pub conflicts: Vec<DependencyConflict>,
}

/// 依赖图节点结构体
///
/// 描述依赖图中的一个节点。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepNode {
    /// 插件唯一标识符
    pub plugin_id: String,
    /// 版本号
    pub version: String,
    /// 是否为根节点
    pub is_root: bool,
}

/// 依赖图边结构体
///
/// 描述依赖图中的边。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepEdge {
    /// 起始节点
    pub from: String,
    /// 目标节点
    pub to: String,
    /// 版本约束
    pub constraint: String,
}

/// 依赖图结构体
///
/// 描述插件的完整依赖关系图。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DependencyGraph {
    /// 图中的节点列表
    pub nodes: Vec<DepNode>,
    /// 图中的边列表
    pub edges: Vec<DepEdge>,
}

/// 默认插件配置
///
/// 定义系统默认插件的配置结构。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultPluginsConfig {
    /// 配置设置
    #[serde(default)]
    pub settings: SettingsConfig,
    /// 必需插件列表
    #[serde(default)]
    pub required: Vec<PluginConfig>,
    /// 可选插件列表
    #[serde(default)]
    pub optional: Vec<PluginConfig>,
}

/// 设置配置结构体
///
/// 定义插件系统的通用设置。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SettingsConfig {
    /// 安装根目录
    #[serde(default = "default_install_root")]
    pub install_root: String,
    /// 临时目录
    #[serde(default = "default_temp_dir")]
    pub temp_dir: String,
    /// 默认数据库 ID
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

/// 插件配置结构体
///
/// 描述单个插件的配置信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginConfig {
    /// 插件唯一标识符
    pub id: String,
    /// 插件版本
    pub version: String,
    /// 插件来源
    pub source: String,
    /// 元数据数据库 ID
    #[serde(default)]
    pub metadata_db_id: Option<String>,
}

/// 初始化结果结构体
///
/// 描述系统插件初始化操作的结果。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InitResult {
    /// 必需插件成功数量
    pub required_succeeded: usize,
    /// 必需插件失败数量
    pub required_failed: usize,
    /// 可选插件成功数量
    pub optional_succeeded: usize,
    /// 可选插件失败数量
    pub optional_failed: usize,
    /// 严重错误列表
    #[serde(default)]
    pub critical_errors: Vec<String>,
    /// 警告信息列表
    #[serde(default)]
    pub warnings: Vec<String>,
}
