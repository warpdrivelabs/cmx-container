//! 卸载服务模块
//!
//! 处理插件卸载流程，提供完整的插件卸载功能。

use std::sync::Arc;

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::error::{PluginError, PluginResult};
use crate::infrastructure::database::repository::PluginRepository;
use crate::infrastructure::database::deployment::DeploymentRepository;
use crate::infrastructure::database::version_history::VersionHistoryRepository;
use crate::infrastructure::cache::layered::LayeredCacheManager;
use crate::infrastructure::messaging::event::{EventBus, Event, EventType};
use crate::audit::logger::AuditLogger;
use crate::core::registry::PluginRegistry;
use crate::core::context::PluginContext;

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
}

/// 卸载服务
pub struct UninstallService {
    deps: UninstallServiceDeps,
}

impl UninstallService {
    /// 创建新的卸载服务
    pub fn new(deps: UninstallServiceDeps) -> Self {
        Self { deps }
    }

    /// 卸载插件（简化版）
    ///
    /// 卸载流程:
    /// 1. 检查插件存在
    /// 2. 检查节点部署记录
    /// 3. 从内存注册表删除
    /// 4. 更新 cmx_plugin_deployments 节点部署记录
    /// 5. 更新 cmx_plugin_versions 版本历史
    /// 6. 检查并更新 cmx_plugin 主表
    /// 7. 更新缓存
    /// 8. 记录审计日志
    /// 9. 发布卸载事件
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
        let existing_deployment = self.deps.deployment_repository
            .find_deployment(&request.plugin_id, &self.deps.node_id)
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

        // 步骤5: 更新 cmx_plugin_deployments 节点部署记录（软删除）
        if let Some(deployment) = existing_deployment {
            let update_fields = crate::infrastructure::database::deployment::DeploymentUpdateFields {
                status: Some("uninstalled".to_string()),
                ..Default::default()
            };
            self.deps.deployment_repository
                .update_deployment(&deployment.id, &update_fields)
                .await?;
        }

        // 步骤6: 更新 cmx_plugin_versions 版本历史
        let current_version = self.deps.version_history_repository
            .get_current_baseline(&plugin_id)
            .await?;

        if let Some(version_record) = current_version {
            let update_fields = crate::infrastructure::database::version_history::VersionHistoryUpdateFields {
                uninstalled_at: Some(Utc::now()),
                ..Default::default()
            };
            self.deps.version_history_repository
                .update_version(&version_record.id, &update_fields)
                .await?;
        }

        // 步骤7: 检查并更新 cmx_plugin 主表
        // 查询是否还有其他节点部署此插件
        let other_deployments = self.deps.deployment_repository
            .list_plugin_deployments(&plugin_id)
            .await?;

        let other_active_nodes: Vec<_> = other_deployments
            .into_iter()
            .filter(|d| d.node_id != self.deps.node_id && d.status != "uninstalled")
            .collect();

        if other_active_nodes.is_empty() {
            // 没有其他节点，更新 cmx_plugin 状态为 uninstalled
            let fields = crate::infrastructure::database::repository::PluginUpdateFields {
                status: Some("uninstalled".to_string()),
                ..Default::default()
            };
            self.deps.repository.update_plugin(&plugin_id, &fields).await?;
        } else {
            // 有其他节点，找到最高版本，更新 cmx_plugin 基线版本
            let highest_version = other_active_nodes
                .iter()
                .map(|d| &d.version)
                .max()
                .cloned();

            if let Some(hv) = highest_version {
                let hv_clone = hv.clone();
                let fields = crate::infrastructure::database::repository::PluginUpdateFields {
                    version: Some(hv_clone.clone()),
                    ..Default::default()
                };
                self.deps.repository.update_plugin(&plugin_id, &fields).await?;

                // 更新 cmx_plugin_versions 的 is_current
                self.deps.version_history_repository
                    .mark_all_not_current(&plugin_id)
                    .await?;

                if let Ok(Some(version_record)) = self.deps.version_history_repository
                    .find_version(&plugin_id, &hv_clone)
                    .await
                {
                    let update_fields = crate::infrastructure::database::version_history::VersionHistoryUpdateFields {
                        is_current: Some(true),
                        ..Default::default()
                    };
                    self.deps.version_history_repository
                        .update_version(&version_record.id, &update_fields)
                        .await?;
                }
            }
        }

        // 步骤8: 清除缓存
        self.deps
            .cache
            .delete(&format!("plugin:{}", plugin_id))
            .await;

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
        self.deps.audit_logger.log(audit_record).await;

        // 步骤10: 发布卸载事件（通知其他节点）
        self.deps
            .event_bus
            .publish(Event::new(
                EventType::PluginUninstalled,
                plugin_id.clone(),
                serde_json::json!({
                    "version": version,
                    "node_id": self.deps.node_id,
                    "remaining_nodes": other_active_nodes.iter().map(|d| d.node_id.clone()).collect::<Vec<_>>(),
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
        })
    }
}
