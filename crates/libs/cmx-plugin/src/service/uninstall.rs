//! 卸载服务模块
//!
//! 处理插件卸载流程，提供完整的插件卸载功能。

use std::sync::Arc;

use crate::infrastructure::cache::layered::LayeredCacheManager;
use crate::infrastructure::database::repository::PluginRepository;
use crate::infrastructure::database::version_history::VersionHistoryRepository;
use crate::audit::logger::AuditLogger;
use crate::core::context::PluginContext;
use crate::core::registry::PluginRegistry;
use serde::{Deserialize, Serialize};

/// 卸载请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UninstallRequest {
    /// 插件ID
    pub plugin_id: String,
    /// 是否强制卸载
    pub force: bool,
    /// 操作者
    pub operator: String,
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

/// 卸载响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UninstallResponse {
    /// 插件ID
    pub plugin_id: String,
    /// 是否成功
    pub success: bool,
    /// 消息
    pub message: String,
}

/// 卸载服务依赖
#[derive(Clone)]
pub struct UninstallServiceDeps {
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
    /// 插件上下文映射
    pub contexts: Arc<tokio::sync::RwLock<std::collections::HashMap<String, PluginContext>>>,
    /// 服务存储
    pub service_storage: Arc<dyn cmx_traits::ServiceStorage>,
    /// 跨实例插件变更通知器
    pub plugin_notifier: Option<Arc<crate::cluster::notification::PluginNotifier>>,
}

/// 卸载服务
#[derive(Clone)]
pub struct UninstallService {
    executor: Arc<crate::service::executor::PluginOperationExecutor>,
}

impl UninstallService {
    /// 创建卸载服务
    pub fn new(executor: Arc<crate::service::executor::PluginOperationExecutor>) -> Self {
        Self { executor }
    }

    /// 卸载插件
    pub async fn uninstall(&self, request: UninstallRequest) -> crate::error::PluginResult<UninstallResponse> {
        self.executor.execute_uninstall(request).await
    }
}
