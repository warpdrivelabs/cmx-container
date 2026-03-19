//! 降级服务模块
//! 
//! 处理插件降级流程

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

/// 降级请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DowngradeRequest {
    /// 插件ID
    pub plugin_id: String,
    /// 目标版本
    pub target_version: String,
    /// 是否强制降级（忽略版本检查）
    pub force: bool,
    /// 是否自动激活
    pub auto_activate: bool,
    /// 是否保留当前版本备份
    pub keep_backup: bool,
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

/// 降级服务
pub struct DowngradeService {
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

impl DowngradeService {
    /// 创建新的降级服务
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
    
    /// 降级插件
    /// 
    /// 完整的降级流程：
    /// 1. 检查插件存在
    /// 2. 查找目标版本备份
    /// 3. 停用当前版本
    /// 4. 创建当前版本备份
    /// 5. 恢复目标版本
    /// 6. 激活目标版本（可选）
    /// 7. 记录审计日志
    pub async fn downgrade(&self, request: DowngradeRequest) -> PluginResult<DowngradeResponse> {
        let start_time = std::time::Instant::now();
        
        // 步骤1：检查插件存在
        let old_plugin = self.repository.find_plugin(&request.plugin_id).await?
            .ok_or_else(|| PluginError::plugin_not_found(&request.plugin_id))?;
        
        let old_version = old_plugin.version.clone();
        let was_activated = old_plugin.status == "activated";
        
        // 步骤2：查找目标版本备份
        let backups = self.backup_manager.list_backups(&request.plugin_id).await
            .map_err(|e| PluginError::Downgrade(format!("获取备份列表失败: {}", e)))?;
        
        let target_backup = backups.into_iter()
            .find(|b| b.version == request.target_version)
            .ok_or_else(|| PluginError::Downgrade(format!(
                "未找到版本 {} 的备份",
                request.target_version
            )))?;
        
        // 步骤3：停用当前版本（如果已激活）
        if was_activated {
            self.activate_service.deactivate(crate::service::activate::DeactivateRequest {
                plugin_id: request.plugin_id.clone(),
                force: request.force,
            }).await?;
        }
        
        // 步骤4：创建当前版本备份
        if request.keep_backup {
            let install_path = PathBuf::from(&old_plugin.install_path);
            if install_path.exists() {
                self.backup_manager.create_backup(
                    &request.plugin_id,
                    &old_version,
                    &install_path,
                ).await.map_err(|e| PluginError::Downgrade(format!("创建备份失败: {}", e)))?;
            }
        }
        
        // 步骤5：恢复目标版本
        let install_path = PathBuf::from(&old_plugin.install_path);
        if install_path.exists() {
            self.storage.remove_dir(&install_path)
                .map_err(|e| PluginError::Downgrade(format!("删除当前版本文件失败: {}", e)))?;
        }
        
        self.backup_manager.restore_backup(&target_backup.path, &install_path).await
            .map_err(|e| PluginError::Downgrade(format!("恢复备份失败: {}", e)))?;
        
        // 更新数据库记录
        let fields = crate::infrastructure::database::repository::PluginUpdateFields {
            version: Some(request.target_version.clone()),
            ..Default::default()
        };
        self.repository.update_plugin(&request.plugin_id, &fields).await?;
        
        // 步骤6：激活目标版本（可选）
        if request.auto_activate || was_activated {
            self.activate_service.activate(crate::service::activate::ActivateRequest {
                plugin_id: request.plugin_id.clone(),
                force: request.force,
            }).await?;
        }
        
        // 步骤7：记录审计日志
        let audit_record = AuditRecord::success(
            request.plugin_id.clone(),
            OperationType::Downgrade,
        ).with_details(serde_json::json!({
            "old_version": old_version,
            "new_version": request.target_version,
            "duration_ms": start_time.elapsed().as_millis(),
        }));
        self.audit_logger.log(audit_record).await;
        
        // 发布事件
        self.event_bus.publish(Event::new(
            EventType::PluginDowngraded,
            request.plugin_id.clone(),
            serde_json::json!({
                "old_version": old_version,
                "new_version": request.target_version,
            }),
        )).await;
        
        Ok(DowngradeResponse {
            plugin_id: request.plugin_id,
            old_version,
            new_version: request.target_version,
            success: true,
            message: "插件降级成功".to_string(),
        })
    }
}
