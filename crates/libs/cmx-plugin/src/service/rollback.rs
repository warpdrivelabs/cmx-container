//! 回滚服务模块
//! 
//! 处理操作回滚流程

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
use crate::audit::record::{AuditRecord, OperationType};
use crate::service::activate::ActivateService;

/// 回滚请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackRequest {
    /// 插件ID
    pub plugin_id: String,
    /// 备份路径（可选，如果不指定则使用最近的备份）
    pub backup_path: Option<PathBuf>,
    /// 是否自动激活
    pub auto_activate: bool,
}

/// 回滚响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackResponse {
    /// 插件ID
    pub plugin_id: String,
    /// 回滚前版本
    pub from_version: String,
    /// 回滚后版本
    pub to_version: String,
    /// 是否成功
    pub success: bool,
    /// 消息
    pub message: String,
}

/// 回滚服务
pub struct RollbackService {
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
    /// 激活服务
    activate_service: Arc<ActivateService>,
}

impl RollbackService {
    /// 创建新的回滚服务
    pub fn new(
        repository: Arc<PluginRepository>,
        cache: Arc<LayeredCacheManager>,
        storage: Arc<FileStorage>,
        backup_manager: Arc<BackupManager>,
        event_bus: Arc<EventBus>,
        audit_logger: Arc<AuditLogger>,
        activate_service: Arc<ActivateService>,
    ) -> Self {
        Self {
            repository,
            cache,
            storage,
            backup_manager,
            event_bus,
            audit_logger,
            activate_service,
        }
    }
    
    /// 回滚插件
    /// 
    /// 完整的回滚流程：
    /// 1. 检查插件存在
    /// 2. 获取备份信息
    /// 3. 停用当前版本
    /// 4. 恢复备份
    /// 5. 更新数据库记录
    /// 6. 激活回滚版本（可选）
    /// 7. 记录审计日志
    pub async fn rollback(&self, request: RollbackRequest) -> PluginResult<RollbackResponse> {
        let start_time = std::time::Instant::now();
        
        // 步骤1：检查插件存在
        let plugin = self.repository.find_plugin(&request.plugin_id).await?
            .ok_or_else(|| PluginError::plugin_not_found(&request.plugin_id))?;
        
        let from_version = plugin.version.clone();
        let was_activated = plugin.status == "activated";
        
        // 步骤2：获取备份信息
        let backup_path = match request.backup_path {
            Some(path) => path,
            None => {
                // 获取最近的备份
                let backups = self.backup_manager.list_backups(&request.plugin_id).await
                    .map_err(|e| PluginError::Rollback(format!("获取备份列表失败: {}", e)))?;
                
                backups.first()
                    .map(|b| b.path.clone())
                    .ok_or_else(|| PluginError::Rollback("没有可用的备份".to_string()))?
            }
        };
        
        // 从备份路径解析版本信息
        let to_version = self.parse_version_from_backup_path(&backup_path)?;
        
        // 步骤3：停用当前版本（如果已激活）
        if was_activated {
            self.activate_service.deactivate(crate::service::activate::DeactivateRequest {
                plugin_id: request.plugin_id.clone(),
                force: false,
            }).await?;
        }
        
        // 步骤4：恢复备份
        let install_path = PathBuf::from(&plugin.install_path);
        
        // 删除当前版本文件
        if install_path.exists() {
            self.storage.remove_dir(&install_path)
                .map_err(|e| PluginError::Rollback(format!("删除当前版本文件失败: {}", e)))?;
        }
        
        // 恢复备份文件
        self.backup_manager.restore_backup(&backup_path, &install_path).await
            .map_err(|e| PluginError::Rollback(format!("恢复备份失败: {}", e)))?;
        
        // 步骤5：更新数据库记录
        self.repository.update_plugin(&request.plugin_id, &crate::infrastructure::database::repository::PluginUpdateFields {
            version: Some(to_version.clone()),
            status: Some("installed".to_string()),
            ..Default::default()
        }).await?;
        
        // 清除缓存
        self.cache.delete(&format!("plugin:{}", request.plugin_id)).await;
        
        // 步骤6：激活回滚版本（可选）
        if request.auto_activate || was_activated {
            self.activate_service.activate(crate::service::activate::ActivateRequest {
                plugin_id: request.plugin_id.clone(),
                force: false,
            }).await?;
        }
        
        // 步骤7：记录审计日志
        let audit_record = AuditRecord::success(
            request.plugin_id.clone(),
            OperationType::Rollback,
        ).with_details(serde_json::json!({
            "from_version": from_version,
            "to_version": to_version,
            "backup_path": backup_path.to_string_lossy().to_string(),
            "duration_ms": start_time.elapsed().as_millis(),
        }));
        self.audit_logger.log(audit_record).await;
        
        // 发布事件
        self.event_bus.publish(Event::new(
            EventType::PluginInstalled, // 使用 Installed 事件表示回滚完成
            request.plugin_id.clone(),
            serde_json::json!({
                "from_version": from_version,
                "to_version": to_version,
                "operation": "rollback",
            }),
        )).await;
        
        Ok(RollbackResponse {
            plugin_id: request.plugin_id,
            from_version,
            to_version,
            success: true,
            message: "插件回滚成功".to_string(),
        })
    }
    
    /// 从备份路径解析版本信息
    fn parse_version_from_backup_path(&self, path: &PathBuf) -> PluginResult<String> {
        // 备份路径格式: {backup_root}/{plugin_id}/{version}_{timestamp}
        let file_name = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        
        // 解析版本（格式：{version}_{timestamp}）
        let parts: Vec<&str> = file_name.splitn(2, '_').collect();
        let version = parts.first().unwrap_or(&"unknown").to_string();
        
        Ok(version)
    }
    
    /// 列出可用的备份
    pub async fn list_available_backups(&self, plugin_id: &str) -> PluginResult<Vec<crate::infrastructure::storage::backup::BackupInfo>> {
        self.backup_manager.list_backups(plugin_id).await
            .map_err(|e| PluginError::Rollback(format!("获取备份列表失败: {}", e)))
    }
    
    /// 删除备份
    pub async fn delete_backup(&self, backup_path: &PathBuf) -> PluginResult<()> {
        self.backup_manager.delete_backup(backup_path).await
            .map_err(|e| PluginError::Rollback(format!("删除备份失败: {}", e)))
    }
    
    /// 清理旧备份
    pub async fn cleanup_old_backups(&self, plugin_id: &str, keep_count: usize) -> PluginResult<usize> {
        self.backup_manager.cleanup_old_backups(plugin_id, keep_count).await
            .map_err(|e| PluginError::Rollback(format!("清理旧备份失败: {}", e)))
    }
}

impl Default for RollbackService {
    fn default() -> Self {
        Self::new(
            Arc::new(PluginRepository::default()),
            Arc::new(LayeredCacheManager::default()),
            Arc::new(FileStorage::new(std::path::Path::new(""))),
            Arc::new(BackupManager::new(PathBuf::from("./backups"))),
            Arc::new(EventBus::new()),
            Arc::new(AuditLogger::default()),
            Arc::new(ActivateService::default()),
        )
    }
}
