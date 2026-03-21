//! 卸载服务模块
//!
//! 处理插件卸载流程

use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::error::{PluginError, PluginResult};
use crate::infrastructure::database::repository::PluginRepository;
use crate::infrastructure::cache::layered::LayeredCacheManager;
use crate::infrastructure::storage::file::FileStorage;
use crate::infrastructure::storage::backup::BackupManager;
use crate::infrastructure::messaging::event::{EventBus, Event, EventType};
use crate::audit::logger::AuditLogger;
use crate::core::registry::PluginRegistry;
use crate::core::context::PluginContext;
use crate::common::{DependencyUtils, DependencyUtilsDeps};

/// 卸载请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UninstallRequest {
    /// 插件ID
    pub plugin_id: String,
    /// 是否强制卸载（忽略依赖检查）
    pub force: bool,
    /// 是否保留配置
    pub keep_config: bool,
    /// 是否保留数据
    pub keep_data: bool,
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
    /// 缓存管理器
    pub cache: Arc<LayeredCacheManager>,
    /// 文件存储
    pub storage: Arc<FileStorage>,
    /// 备份管理器
    pub backup_manager: Arc<BackupManager>,
    /// 事件总线
    pub event_bus: Arc<EventBus>,
    /// 审计日志
    pub audit_logger: Arc<AuditLogger>,
    /// 插件注册表
    pub registry: Arc<tokio::sync::RwLock<PluginRegistry>>,
    /// 插件上下文
    pub contexts: Arc<tokio::sync::RwLock<std::collections::HashMap<String, PluginContext>>>,
}

/// 卸载服务
pub struct UninstallService {
    deps: UninstallServiceDeps,
    dependency_utils: DependencyUtils,
}

impl UninstallService {
    /// 创建新的卸载服务
    pub fn new(deps: UninstallServiceDeps) -> Self {
        let dependency_utils = DependencyUtils::new(DependencyUtilsDeps {
            repository: deps.repository.clone(),
        });
        Self { deps, dependency_utils }
    }

    /// 卸载插件
    ///
    /// 完整的卸载流程：
    /// 1. 检查插件存在
    /// 2. 检查依赖（非强制模式）
    /// 3. 停用插件（如果已激活）
    /// 4. 创建备份（如果保留数据）
    /// 5. 删除文件
    /// 6. 清理数据库记录
    /// 7. 更新注册表和上下文
    /// 8. 清除缓存
    /// 9. 记录审计日志
    pub async fn uninstall(&self, request: UninstallRequest) -> PluginResult<UninstallResponse> {
        let start_time = std::time::Instant::now();

        let plugin = self
            .deps
            .repository
            .find_plugin(&request.plugin_id)
            .await?
            .ok_or_else(|| PluginError::plugin_not_found(&request.plugin_id))?;

        if !request.force {
            let dependents = self.dependency_utils.check_dependents(&request.plugin_id).await?;
            if !dependents.is_empty() {
                return Err(PluginError::Dependency(format!(
                    "插件 {} 被以下插件依赖: {}",
                    request.plugin_id,
                    dependents.join(", ")
                )));
            }
        }

        if plugin.status == "activated" {
            self.deactivate_plugin(&request.plugin_id).await?;
        }

        if request.keep_data {
            let install_path = PathBuf::from(&plugin.install_path);
            if install_path.exists() {
                self.deps
                    .backup_manager
                    .create_backup(&request.plugin_id, &plugin.version, &install_path)
                    .await
                    .map_err(|e| PluginError::Uninstall(format!("创建备份失败: {}", e)))?;
            }
        }

        let install_path = PathBuf::from(&plugin.install_path);
        if install_path.exists() && !request.keep_config {
            self.deps
                .storage
                .remove_dir(&install_path)
                .map_err(|e| PluginError::Uninstall(format!("删除插件文件失败: {}", e)))?;
        }

        self.deps
            .repository
            .delete_plugin(&request.plugin_id)
            .await?;

        {
            let mut registry = self.deps.registry.write().await;
            registry.unregister(&request.plugin_id);
        }

        {
            let mut contexts = self.deps.contexts.write().await;
            contexts.remove(&request.plugin_id);
        }

        self.deps
            .cache
            .delete(&format!("plugin:{}", request.plugin_id))
            .await;

        let audit_record = crate::audit::record::AuditRecord::success(
            request.plugin_id.clone(),
            crate::audit::record::OperationType::Uninstall,
        )
        .with_details(serde_json::json!({
            "version": plugin.version,
            "keep_config": request.keep_config,
            "keep_data": request.keep_data,
            "duration_ms": start_time.elapsed().as_millis(),
        }));
        self.deps.audit_logger.log(audit_record).await;

        self.deps
            .event_bus
            .publish(Event::new(
                EventType::PluginUninstalled,
                request.plugin_id.clone(),
                serde_json::json!({
                    "version": plugin.version,
                }),
            ))
            .await;

        Ok(UninstallResponse {
            plugin_id: request.plugin_id,
            success: true,
            message: "插件卸载成功".to_string(),
        })
    }

    /// 停用插件
    async fn deactivate_plugin(&self, plugin_id: &str) -> PluginResult<()> {
        self.deps
            .repository
            .update_plugin_status(plugin_id, "deactivated")
            .await?;
        tracing::info!("插件已停用: {}", plugin_id);
        Ok(())
    }
}

impl Default for UninstallService {
    fn default() -> Self {
        use std::sync::Arc;

        Self::new(UninstallServiceDeps {
            repository: Arc::new(PluginRepository::default()),
            cache: Arc::new(LayeredCacheManager::default()),
            storage: Arc::new(FileStorage::new(std::path::Path::new(""))),
            backup_manager: Arc::new(BackupManager::new(PathBuf::from("./backups"))),
            event_bus: Arc::new(EventBus::new()),
            audit_logger: Arc::new(AuditLogger::default()),
            registry: Arc::new(tokio::sync::RwLock::new(PluginRegistry::new())),
            contexts: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
        })
    }
}
