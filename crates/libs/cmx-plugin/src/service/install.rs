//! 安装服务模块
//!
//! 处理插件安装流程

use std::path::PathBuf;
use std::sync::Arc;

use crate::domain::plugin::PluginSource;
use crate::error::PluginResult;
use crate::infrastructure::cache::layered::LayeredCacheManager;
use crate::infrastructure::database::repository::PluginRepository;
use crate::infrastructure::database::version_history::VersionHistoryRepository;
use crate::infrastructure::storage::backup::BackupManager;
use crate::infrastructure::storage::file::FileStorage;
use crate::audit::logger::AuditLogger;
use crate::core::context::PluginContext;
use crate::core::registry::PluginRegistry;
use crate::security::validator::SecurityValidator;
use crate::service::executor::PluginOperationExecutor;
use cmx_buffer::LockManager;
use cmx_traits::ServiceQuery;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

/// 安装请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallRequest {
    /// 插件来源
    pub source: PluginSource,
    /// 目标数据库ID（可选）
    pub db_id: Option<String>,
    /// 是否自动激活
    pub auto_activate: bool,
    /// 版本约束（仅对注册表来源有效，如 "^1.0.0", ">=2.0.0"）
    #[serde(default)]
    pub version_constraint: Option<String>,
    /// 构建类型 debug release
    pub build_type: Option<String>,
    /// 市场版本来源 ID，关联 `cmx_marketplace_plugin_version.id`。
    pub marketplace_source_id: Option<String>,
    /// 应用ID
    #[serde(default)]
    pub app_id: Option<String>,
}

/// 安装响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallResponse {
    /// 插件ID
    pub plugin_id: String,
    /// 安装路径
    pub install_path: PathBuf,
    /// 插件版本
    pub version: String,
    /// 是否成功
    pub success: bool,
    /// 消息
    pub message: String,
}

/// 安装服务依赖
#[derive(Clone)]
pub struct InstallServiceDeps {
    /// 数据仓库
    pub repository: Arc<PluginRepository>,
    /// 版本历史仓库
    pub version_history_repository: Arc<VersionHistoryRepository>,
    /// 缓存管理器
    pub cache: Arc<LayeredCacheManager>,
    /// 文件存储
    pub storage: Arc<FileStorage>,
    /// 备份管理器
    pub backup_manager: Arc<BackupManager>,
    /// 安全验证器
    pub security_validator: Arc<SecurityValidator>,
    /// 审计日志
    pub audit_logger: Arc<AuditLogger>,
    /// 插件注册表
    pub registry: Arc<RwLock<PluginRegistry>>,
    /// 插件上下文
    pub contexts: Arc<RwLock<std::collections::HashMap<String, PluginContext>>>,
    /// 安装根目录
    pub plugin_root: PathBuf,
    /// 临时目录
    pub temp_root: PathBuf,
    /// 默认数据库ID
    pub default_database_id: String,
    /// 节点名称
    pub node_name: Option<String>,
    /// 节点类型
    pub node_type: Option<String>,
    /// 服务存储
    pub service_storage: Arc<dyn cmx_traits::ServiceStorage>,
    /// 服务查询（用于降级场景查询插件的服务定义）
    pub service_query: Arc<dyn ServiceQuery>,
    /// 跨实例变更通知器
    pub plugin_notifier: Option<Arc<crate::cluster::notification::PluginNotifier>>,
    /// 分布式锁管理器
    pub lock_manager: Option<Arc<LockManager>>,
}

/// 安装服务
#[derive(Clone)]
pub struct InstallService {
    executor: Arc<PluginOperationExecutor>,
}

impl InstallService {
    /// 创建新的安装服务
    pub fn new(executor: Arc<PluginOperationExecutor>) -> Self {
        Self { executor }
    }

    /// 执行安装操作
    pub async fn install(&self, request: InstallRequest) -> PluginResult<InstallResponse> {
        self.executor.execute_install(request).await
    }
}
