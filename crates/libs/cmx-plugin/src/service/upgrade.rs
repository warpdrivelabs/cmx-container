//! 升级服务模块
//!
//! 处理插件升级流程，提供完整的插件版本升级功能。

use std::path::PathBuf;
use std::sync::Arc;

use crate::infrastructure::cache::layered::LayeredCacheManager;
use crate::infrastructure::database::repository::PluginRepository;
use crate::infrastructure::database::version_history::VersionHistoryRepository;
use crate::infrastructure::storage::backup::BackupManager;
use crate::infrastructure::storage::file::FileStorage;
use crate::security::validator::SecurityValidator;
use crate::audit::logger::AuditLogger;
use crate::core::context::PluginContext;
use crate::core::registry::PluginRegistry;
use crate::domain::plugin::PluginSource;
use cmx_buffer::LockManager;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

/// 升级请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpgradeRequest {
    /// 插件ID
    pub plugin_id: String,
    /// 新版本来源
    pub source: PluginSource,
    /// 版本约束
    #[serde(default)]
    pub version_constraint: Option<String>,
    /// 是否强制升级（忽略版本检查）
    pub force: bool,
    /// 操作者
    pub operator: Option<String>,
    /// 构建类型 debug release
    pub  build_type : Option<String>,
    /// 市场版本来源 ID，关联 `cmx_marketplace_plugin_version.id`。
    pub marketplace_source_id: Option<String>,
    /// 应用ID
    #[serde(default)]
    pub app_id: Option<String>,
    /// 是否发送事件通知（管控接口调用时设为 false）
    #[serde(default = "default_true")]
    pub send_event: bool,
}

fn default_true() -> bool {
    true
}

/// 升级响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpgradeResponse {
    /// 插件ID
    pub plugin_id: String,
    /// 旧版本
    pub old_version: String,
    /// 新版本
    pub new_version: String,
    /// 是否成功
    pub success: bool,
    /// 消息
    pub message: String,
}

/// 升级服务依赖
#[derive(Clone)]
pub struct UpgradeServiceDeps {
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
    /// 插件上下文映射
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
    /// 跨实例插件变更通知器
    pub plugin_notifier: Option<Arc<crate::cluster::notification::PluginNotifier>>,
    /// 分布式锁管理器
    pub lock_manager: Option<Arc<LockManager>>,
}

/// 升级服务
#[derive(Clone)]
pub struct UpgradeService {
    executor: Arc<crate::service::executor::PluginOperationExecutor>,
}

impl UpgradeService {
    /// 创建新的升级服务
    pub fn new(executor: Arc<crate::service::executor::PluginOperationExecutor>) -> Self {
        Self { executor }
    }

    /// 执行升级操作
    pub async fn upgrade(&self, request: UpgradeRequest) -> crate::error::PluginResult<UpgradeResponse> {
        self.executor.execute_upgrade(request).await
    }
}
