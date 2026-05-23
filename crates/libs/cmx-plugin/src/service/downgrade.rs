//! 降级服务模块
//!
//! 处理插件降级流程，提供将插件回退到指定旧版本的功能。
//!
//! 降级只是切换版本目录，不涉及文件拷贝。

use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use cmx_traits::{ServiceQuery, ServiceStorage};
use crate::domain::plugin::PluginSource;
use crate::infrastructure::cache::layered::LayeredCacheManager;
use crate::infrastructure::database::repository::PluginRepository;
use crate::infrastructure::database::version_history::VersionHistoryRepository;
use crate::audit::logger::AuditLogger;
use crate::core::registry::PluginRegistry;

/// 降级请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DowngradeRequest {
    /// 插件ID
    pub plugin_id: String,
    /// 目标版本
    pub target_version: String,
    /// 插件来源（可选，用于下载旧版本）
    pub source: Option<PluginSource>,
    /// 操作者
    pub operator: Option<String>,
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

/// 降级响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DowngradeResponse {
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

/// 降级服务依赖
#[derive(Clone)]
pub struct DowngradeServiceDeps {
    /// 数据仓库
    pub repository: Arc<PluginRepository>,
    /// 版本历史仓库
    pub version_history_repository: Arc<VersionHistoryRepository>,
    /// 缓存管理器
    pub cache: Arc<LayeredCacheManager>,
    /// 审计日志
    pub audit_logger: Arc<AuditLogger>,
    /// 插件注册表
    pub registry: Arc<tokio::sync::RwLock<PluginRegistry>>,
    /// 安装根目录
    pub plugin_root: PathBuf,
    /// 默认数据库ID
    pub default_database_id: String,
    /// 服务查询（用于查询插件的服务定义）
    pub service_query: Arc<dyn ServiceQuery>,
    /// 服务存储（用于更新服务定义版本）
    pub service_storage: Arc<dyn ServiceStorage>,
    /// 跨实例插件变更通知器
    pub plugin_notifier: Option<Arc<crate::cluster::notification::PluginNotifier>>,

}

/// 降级服务
#[derive(Clone)]
pub struct DowngradeService {
    executor: Arc<crate::service::executor::PluginOperationExecutor>,
}

impl DowngradeService {
    /// 创建新的降级服务
    pub fn new(executor: Arc<crate::service::executor::PluginOperationExecutor>) -> Self {
        Self { executor }
    }

    /// 降级插件
    pub async fn downgrade(&self, request: DowngradeRequest) -> crate::error::PluginResult<DowngradeResponse> {
        self.executor.execute_downgrade(request).await
    }
}


