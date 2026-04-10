//! 卸载服务模块
//!
//! 处理插件卸载流程，提供完整的插件卸载功能。

use std::path::Path;
use std::sync::Arc;

use cmx_database::get_default_db_manager;
use crate::audit::logger::AuditLogger;
use crate::core::context::PluginContext;
use crate::core::registry::PluginRegistry;
use crate::error::{PluginError, PluginResult};
use crate::infrastructure::cache::layered::LayeredCacheManager;
use crate::infrastructure::database::deployment::DeploymentRepository;
use crate::infrastructure::database::repository::PluginRepository;
use crate::infrastructure::database::version_history::VersionHistoryRepository;
use crate::infrastructure::messaging::event::{Event, EventBus, EventType};
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};

/// 卸载请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UninstallRequest {
    /// 插件ID
    pub plugin_id: String,
    /// 是否强制卸载
    pub force: bool,
    /// 操作者
    pub operator: String,
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
    /// 部署仓库
    pub deployment_repository: Arc<DeploymentRepository>,
    /// 版本历史仓库
    pub version_history_repository: Arc<VersionHistoryRepository>,
    /// 缓存管理器
    pub cache: Arc<LayeredCacheManager>,
    /// 事件总线
    pub event_bus: Arc<EventBus>,
    /// 审计日志
    pub audit_logger: Arc<AuditLogger>,
    /// 插件注册表
    pub registry: Arc<tokio::sync::RwLock<PluginRegistry>>,
    /// 插件上下文映射
    pub contexts: Arc<tokio::sync::RwLock<std::collections::HashMap<String, PluginContext>>>,
    /// 节点ID
    pub node_id: String,
    /// 服务存储
    pub service_storage: Arc<dyn cmx_traits::ServiceStorage>,
}

/// 卸载服务
#[derive(Clone)]
pub struct UninstallService {
    deps: UninstallServiceDeps,
}

impl UninstallService {
    /// 创建卸载服务
    pub fn new(deps: UninstallServiceDeps) -> Self {
        Self { deps }
    }

    /// 卸载插件
    ///
    /// 卸载流程:
    /// 1. 检查插件存在
    /// 2. 检查节点部署记录
    /// 3. 从内存注册表删除
    /// 4. 清除插件上下文
    /// 5. 物理删除 cmx_plugin_deployments 部署记录
    /// 6. 物理删除 cmx_plugin_versions 版本历史记录
    /// 7. 物理删除 cmx_plugin 主表记录
    /// 8. 清除缓存
    /// 9. 记录审计日志
    /// 10. 发布卸载事件
    pub async fn uninstall(&self, request: UninstallRequest) -> PluginResult<UninstallResponse> {
        let start_time = std::time::Instant::now();

        // 步骤1: 检查插件存在
        let plugin = self
            .deps
            .repository
            .find_plugin(&request.plugin_id)
            .await?
            .ok_or_else(|| PluginError::plugin_not_found(&request.plugin_id))?;

        let version = plugin.version.clone();

        // 步骤2: 检查节点部署记录
        let existing_deployment = self
            .deps
            .deployment_repository
            .find_deployment(&request.plugin_id, &self.deps.node_id, &version)
            .await?;

        if existing_deployment.is_none() {
            return Err(PluginError::invalid_state(
                &request.plugin_id,
                "not_deployed",
                "节点未部署此插件",
            ));
        }

        let plugin_id = request.plugin_id.clone();

        // 步骤3: 从内存注册表删除
        {
            let mut registry = self.deps.registry.write().await;
            registry.unregister(&plugin_id);
        }

        // 步骤4: 更新上下文
        {
            let mut contexts = self.deps.contexts.write().await;
            contexts.remove(&plugin_id);
        }

        // 步骤5: 物理删除 cmx_plugin_deployments 部署记录
        self.deps
            .deployment_repository
            .delete_deployments_by_plugin_id(&plugin_id, None)
            .await?;

        // 步骤6: 物理删除 cmx_plugin_versions 版本历史记录
        self.deps
            .version_history_repository
            .delete_versions_by_plugin_id(&plugin_id, None)
            .await?;

        // 步骤7: 物理删除 cmx_plugin 主表记录
        self.deps.repository.delete_plugin(&plugin_id).await?;

        // 步骤7.1: 物理删除 cmx_meta_table_define 和 cmx_meta_table_define_version 对应 plugin_id 的数据
        {
            let dbm = get_default_db_manager();
            let default_db_id = dbm.get_default_db_id().await;
            crate::infrastructure::database::table_metadata::TableMetadataService::delete_by_plugin_id(
                dbm,
                default_db_id.as_str(),
                None,
                &plugin_id,
            )
            .await
            .map_err(|e| PluginError::Database(format!("删除表元数据失败: {}", e)))?;
        }
        // 7.2: 清理此插件关联的服务定义
        if let Err(e) = self.deps.service_storage.delete_services_by_plugin(&plugin_id).await {
            warn!("清理插件 {} 的服务定义失败: {:?}", plugin_id, e);
        } else {
            info!("已清理插件 {} 的服务定义", plugin_id);
        }
        // 步骤8: 清除缓存
        self.deps
            .cache
            .delete(&format!("plugin:{}", plugin_id))
            .await;

        //移除物理安装目录
        let install_path = &plugin.install_path;
        if let Some(parent_path) = Path::new(install_path).parent().map(|p| p.to_string_lossy().to_string()) {
            if std::fs::remove_dir_all(&parent_path).is_ok() {
                info!("删除插件安装目录成功: {}", parent_path);
            } else {
                error!("删除插件安装目录失败: {}", parent_path);
            }
        }


        // 步骤9: 记录审计日志
        let duration_ms = start_time.elapsed().as_millis() as i64;
        let audit_record = crate::audit::record::AuditRecord::success(
            plugin_id.clone(),
            crate::audit::record::OperationType::Uninstall,
        )
        .with_details(serde_json::json!({
            "version": version,
            "node_id": self.deps.node_id,
        }))
        .with_old_value(version.clone())
        .with_completed(duration_ms);
        let _ = self.deps.audit_logger.log(audit_record).await;



        // 步骤10.2: 发布卸载事件（通知其他节点）
        self.deps
            .event_bus
            .publish(Event::new(
                EventType::PluginUninstalled,
                plugin_id.clone(),
                serde_json::json!({
                    "version": version,
                    "node_id": self.deps.node_id,
                }),
            ))
            .await;



        Ok(UninstallResponse {
            plugin_id,
            success: true,
            message: "插件卸载成功".to_string(),
        })
    }
}

impl Default for UninstallService {
    fn default() -> Self {
        use std::sync::Arc;
        use cmx_service::ServiceStorageImpl;
        use cmx_database::get_default_db_manager;
        use cmx_service::ServiceRepository;

        let db_manager = get_default_db_manager();
        let default_database_id = "primary".to_string();

        let repository = Arc::new(ServiceRepository::new(db_manager.clone(),default_database_id));
        let service_storage: Arc<dyn cmx_traits::ServiceStorage> = Arc::new(ServiceStorageImpl::new(repository));

        Self::new(UninstallServiceDeps {
            repository: Arc::new(PluginRepository::default()),
            deployment_repository: Arc::new(DeploymentRepository::default()),
            version_history_repository: Arc::new(VersionHistoryRepository::default()),
            cache: Arc::new(LayeredCacheManager::default()),
            event_bus: Arc::new(EventBus::new()),
            audit_logger: Arc::new(AuditLogger::default()),
            registry: Arc::new(tokio::sync::RwLock::new(PluginRegistry::new())),
            contexts: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            node_id: "default".to_string(),
            service_storage,
        })
    }
}
