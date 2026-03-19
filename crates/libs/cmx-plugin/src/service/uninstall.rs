//! 卸载服务模块
//! 
//! 处理插件卸载流程

use std::path::PathBuf;
use std::sync::Arc;
use serde::{Deserialize, Serialize};

use crate::error::{PluginError, PluginResult};
use crate::domain::plugin::PluginStatus;
use crate::infrastructure::database::repository::{PluginRepository, PluginUpdateFields};
use crate::infrastructure::cache::layered::LayeredCacheManager;
use crate::infrastructure::storage::file::FileStorage;
use crate::infrastructure::storage::backup::BackupManager;
use crate::infrastructure::messaging::event::{EventBus, Event, EventType};
use crate::audit::logger::AuditLogger;
use crate::audit::record::{AuditRecord, OperationType};

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

/// 卸载服务
pub struct UninstallService {
    /// 数据仓库
    repository: Arc<PluginRepository>,
    /// 缓存管理器
    cache: Arc<LayeredCacheManager>,
    /// 文件存储
    storage: Arc<FileStorage>,
    /// 备份管理器
    backup_manager: Arc<BackupManager>,
    /// 事件总线
    event_bus: Arc<EventBus>,
    /// 审计日志
    audit_logger: Arc<AuditLogger>,
}

impl UninstallService {
    /// 创建新的卸载服务
    pub fn new(
        repository: Arc<PluginRepository>,
        cache: Arc<LayeredCacheManager>,
        storage: Arc<FileStorage>,
        backup_manager: Arc<BackupManager>,
        event_bus: Arc<EventBus>,
        audit_logger: Arc<AuditLogger>,
    ) -> Self {
        Self {
            repository,
            cache,
            storage,
            backup_manager,
            event_bus,
            audit_logger,
        }
    }
    
    /// 卸载插件
    /// 
    /// 完整的卸载流程：
    /// 1. 检查插件存在
    /// 2. 检查依赖
    /// 3. 停用插件
    /// 4. 创建备份（可选）
    /// 5. 删除文件
    /// 6. 清理数据库记录
    /// 7. 清除缓存
    /// 8. 记录审计日志
    pub async fn uninstall(&self, request: UninstallRequest) -> PluginResult<UninstallResponse> {
        let start_time = std::time::Instant::now();
        
        // 步骤1：检查插件存在
        let plugin = self.repository.find_plugin(&request.plugin_id).await?
            .ok_or_else(|| PluginError::plugin_not_found(&request.plugin_id))?;
        
        // 步骤2：检查依赖（非强制模式）
        if !request.force {
            let dependents = self.check_dependents(&request.plugin_id).await?;
            if !dependents.is_empty() {
                return Err(PluginError::Dependency(format!(
                    "插件 {} 被以下插件依赖: {}",
                    request.plugin_id,
                    dependents.join(", ")
                )));
            }
        }
        
        // 步骤3：停用插件（如果已激活）
        if plugin.status == "activated" {
            // 停用插件
            self.deactivate_plugin(&request.plugin_id).await?;
        }
        
        // 步骤4：创建备份（如果保留数据）
        if request.keep_data {
            let install_path = PathBuf::from(&plugin.install_path);
            if install_path.exists() {
                self.backup_manager.create_backup(
                    &request.plugin_id,
                    &plugin.version,
                    &install_path,
                ).await.map_err(|e| PluginError::Uninstall(format!("创建备份失败: {}", e)))?;
            }
        }
        
        // 步骤5：删除文件
        let install_path = PathBuf::from(&plugin.install_path);
        if install_path.exists() && !request.keep_config {
            self.storage.remove_dir(&install_path)
                .map_err(|e| PluginError::Uninstall(format!("删除插件文件失败: {}", e)))?;
        }
        
        // 步骤6：清理数据库记录
        self.repository.delete_plugin(&request.plugin_id).await?;
        
        // 步骤7：清除缓存
        self.cache.delete(&format!("plugin:{}", request.plugin_id)).await;
        
        // 步骤8：记录审计日志
        let audit_record = AuditRecord::success(
            request.plugin_id.clone(),
            OperationType::Uninstall,
        ).with_details(serde_json::json!({
            "version": plugin.version,
            "keep_config": request.keep_config,
            "keep_data": request.keep_data,
            "duration_ms": start_time.elapsed().as_millis(),
        }));
        self.audit_logger.log(audit_record).await;
        
        // 发布事件
        self.event_bus.publish(Event::new(
            EventType::PluginUninstalled,
            request.plugin_id.clone(),
            serde_json::json!({
                "version": plugin.version,
            }),
        )).await;
        
        Ok(UninstallResponse {
            plugin_id: request.plugin_id,
            success: true,
            message: "插件卸载成功".to_string(),
        })
    }
    
    /// 检查依赖此插件的其他插件
    /// 
    /// 查询所有插件，检查它们的依赖列表中是否包含当前插件。
    async fn check_dependents(&self, plugin_id: &str) -> PluginResult<Vec<String>> {
        let all_plugins = self.repository.list_plugins(&crate::domain::plugin::PluginFilter::default()).await?;
        let mut dependents = Vec::new();
        
        for plugin in all_plugins {
            // 从元数据中获取依赖信息
            if let Some(ref metadata) = plugin.metadata {
                if let Some(deps) = metadata.get("dependencies").and_then(|d| d.as_array()) {
                    for dep in deps {
                        if let Some(dep_id) = dep.get("plugin_id").and_then(|id| id.as_str()) {
                            if dep_id == plugin_id {
                                dependents.push(plugin.plugin_id.clone());
                                break;
                            }
                        }
                    }
                }
            }
        }
        
        Ok(dependents)
    }
    
    /// 停用插件
    /// 
    /// 更新插件状态为已停用。
    async fn deactivate_plugin(&self, plugin_id: &str) -> PluginResult<()> {
        self.repository.update_plugin_status(plugin_id, "deactivated").await?;
        tracing::info!("插件已停用: {}", plugin_id);
        Ok(())
    }
    
    /// 检查插件是否已安装
    pub async fn is_installed(&self, plugin_id: &str) -> PluginResult<bool> {
        self.repository.plugin_exists(plugin_id).await
    }
}

impl Default for UninstallService {
    fn default() -> Self {
        Self::new(
            Arc::new(PluginRepository::default()),
            Arc::new(LayeredCacheManager::default()),
            Arc::new(FileStorage::new(std::path::Path::new(""))),
            Arc::new(BackupManager::new(PathBuf::from("./backups"))),
            Arc::new(EventBus::new()),
            Arc::new(AuditLogger::default()),
        )
    }
}
