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
use crate::core::context::PluginContext;

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

/// 降级服务依赖
pub struct DowngradeServiceDeps {
    /// 数据仓库
    pub repository: Arc<PluginRepository>,
    /// 文件存储
    pub storage: Arc<FileStorage>,
    /// 备份管理器
    pub backup_manager: Arc<BackupManager>,
    /// 事件总线
    pub event_bus: Arc<EventBus>,
    /// 审计日志
    pub audit_logger: Arc<AuditLogger>,
    /// 插件上下文
    pub contexts: Arc<tokio::sync::RwLock<std::collections::HashMap<String, PluginContext>>>,
}

/// 降级服务
pub struct DowngradeService {
    deps: DowngradeServiceDeps,
}

impl DowngradeService {
    /// 创建新的降级服务
    pub fn new(deps: DowngradeServiceDeps) -> Self {
        Self { deps }
    }

    /// 降级插件
    ///
    /// 完整的降级流程：
    /// 1. 检查插件存在
    /// 2. 查找目标版本备份
    /// 3. 停用当前版本
    /// 4. 创建当前版本备份
    /// 5. 恢复目标版本
    /// 6. 更新数据库记录
    /// 7. 重新激活（如果需要）
    /// 8. 记录审计日志
    pub async fn downgrade(&self, request: DowngradeRequest) -> PluginResult<DowngradeResponse> {
        let start_time = std::time::Instant::now();

        // 步骤1：检查插件存在
        let plugin = self
            .deps
            .repository
            .find_plugin(&request.plugin_id)
            .await?
            .ok_or_else(|| PluginError::plugin_not_found(&request.plugin_id))?;

        let current_version = plugin.version.clone();

        // 步骤2：查找目标版本的备份
        let backups = self
            .deps
            .backup_manager
            .list_backups(&request.plugin_id)
            .await
            .map_err(|e| PluginError::Downgrade(format!("获取备份列表失败: {}", e)))?;

        let target_backup = backups
            .into_iter()
            .find(|b| b.version == request.target_version)
            .ok_or_else(|| {
                PluginError::Downgrade(format!("未找到版本 {} 的备份", request.target_version))
            })?;

        // 步骤3：创建当前版本备份
        let install_path = PathBuf::from(&plugin.install_path);
        self.deps
            .backup_manager
            .create_backup(&request.plugin_id, &current_version, &install_path)
            .await
            .map_err(|e| PluginError::Downgrade(format!("创建备份失败: {}", e)))?;

        // 步骤4：停用插件（如果已激活）
        let was_activated = plugin.status == "activated";
        if was_activated {
            self.deps
                .repository
                .update_plugin_status(&request.plugin_id, "deactivated")
                .await?;
        }

        // 步骤5：恢复旧版本
        if install_path.exists() {
            self.deps
                .storage
                .remove_dir(&install_path)
                .map_err(|e| PluginError::Downgrade(format!("删除当前文件失败: {}", e)))?;
        }

        self.deps
            .backup_manager
            .restore_backup(&target_backup.path, &install_path)
            .await
            .map_err(|e| PluginError::Downgrade(format!("恢复备份失败: {}", e)))?;

        // 步骤6：更新数据库记录
        let fields = crate::infrastructure::database::repository::PluginUpdateFields {
            version: Some(request.target_version.clone()),
            status: Some(if was_activated || request.auto_activate {
                "activated".to_string()
            } else {
                "installed".to_string()
            }),
            ..Default::default()
        };
        self.deps
            .repository
            .update_plugin(&request.plugin_id, &fields)
            .await?;

        // 更新上下文
        {
            let mut contexts = self.deps.contexts.write().await;
            if let Some(context) = contexts.get_mut(&request.plugin_id) {
                context.version = request.target_version.clone();
            }
        }

        // 步骤7：记录审计日志
        let audit_record = crate::audit::record::AuditRecord::success(
            request.plugin_id.clone(),
            crate::audit::record::OperationType::Downgrade,
        )
        .with_details(serde_json::json!({
            "from_version": current_version,
            "to_version": request.target_version,
            "duration_ms": start_time.elapsed().as_millis(),
        }));
        self.deps.audit_logger.log(audit_record).await;

        // 发布事件
        self.deps
            .event_bus
            .publish(Event::new(
                EventType::PluginDowngraded,
                request.plugin_id.clone(),
                serde_json::json!({
                    "from_version": current_version,
                    "to_version": request.target_version,
                }),
            ))
            .await;

        Ok(DowngradeResponse {
            plugin_id: request.plugin_id,
            old_version: current_version,
            new_version: request.target_version,
            success: true,
            message: "插件降级成功".to_string(),
        })
    }
}

impl Default for DowngradeService {
    fn default() -> Self {
        use std::sync::Arc;

        Self::new(DowngradeServiceDeps {
            repository: Arc::new(PluginRepository::default()),
            storage: Arc::new(FileStorage::new(std::path::Path::new(""))),
            backup_manager: Arc::new(BackupManager::new(PathBuf::from("./backups"))),
            event_bus: Arc::new(EventBus::new()),
            audit_logger: Arc::new(AuditLogger::default()),
            contexts: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
        })
    }
}
