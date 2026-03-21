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
use crate::core::context::PluginContext;

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

/// 回滚服务依赖
pub struct RollbackServiceDeps {
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
    /// 插件上下文
    pub contexts: Arc<tokio::sync::RwLock<std::collections::HashMap<String, PluginContext>>>,
}

/// 回滚服务
pub struct RollbackService {
    deps: RollbackServiceDeps,
}

impl RollbackService {
    /// 创建新的回滚服务
    pub fn new(deps: RollbackServiceDeps) -> Self {
        Self { deps }
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
        let plugin = self
            .deps
            .repository
            .find_plugin(&request.plugin_id)
            .await?
            .ok_or_else(|| PluginError::plugin_not_found(&request.plugin_id))?;

        let from_version = plugin.version.clone();
        let was_activated = plugin.status == "activated";

        // 步骤2：获取备份信息
        let backup_path = match request.backup_path {
            Some(path) => path,
            None => {
                let backups = self
                    .deps
                    .backup_manager
                    .list_backups(&request.plugin_id)
                    .await
                    .map_err(|e| PluginError::Rollback(format!("获取备份列表失败: {}", e)))?;

                backups
                    .first()
                    .map(|b| b.path.clone())
                    .ok_or_else(|| PluginError::Rollback("没有可用的备份".to_string()))?
            }
        };

        let to_version = self.parse_version_from_backup_path(&backup_path)?;

        // 步骤3：停用当前版本（如果已激活）
        if was_activated {
            self.deps
                .repository
                .update_plugin_status(&request.plugin_id, "deactivated")
                .await?;
        }

        // 步骤4：恢复备份
        let install_path = PathBuf::from(&plugin.install_path);

        if install_path.exists() {
            self.deps
                .storage
                .remove_dir(&install_path)
                .map_err(|e| PluginError::Rollback(format!("删除当前版本文件失败: {}", e)))?;
        }

        self.deps
            .backup_manager
            .restore_backup(&backup_path, &install_path)
            .await
            .map_err(|e| PluginError::Rollback(format!("恢复备份失败: {}", e)))?;

        // 步骤5：更新数据库记录
        self.deps
            .repository
            .update_plugin(
                &request.plugin_id,
                &crate::infrastructure::database::repository::PluginUpdateFields {
                    version: Some(to_version.clone()),
                    status: Some(if request.auto_activate || was_activated {
                        "activated".to_string()
                    } else {
                        "installed".to_string()
                    }),
                    ..Default::default()
                },
            )
            .await?;

        // 清除缓存
        self.deps
            .cache
            .delete(&format!("plugin:{}", request.plugin_id))
            .await;

        // 更新上下文
        {
            let mut contexts = self.deps.contexts.write().await;
            if let Some(context) = contexts.get_mut(&request.plugin_id) {
                context.version = to_version.clone();
            }
        }

        // 步骤6：记录审计日志
        let audit_record = crate::audit::record::AuditRecord::success(
            request.plugin_id.clone(),
            crate::audit::record::OperationType::Rollback,
        )
        .with_details(serde_json::json!({
            "from_version": from_version,
            "to_version": to_version,
            "backup_path": backup_path.to_string_lossy().to_string(),
            "duration_ms": start_time.elapsed().as_millis(),
        }));
        self.deps.audit_logger.log(audit_record).await;

        // 发布事件
        self.deps
            .event_bus
            .publish(Event::new(
                EventType::PluginInstalled,
                request.plugin_id.clone(),
                serde_json::json!({
                    "from_version": from_version,
                    "to_version": to_version,
                    "operation": "rollback",
                }),
            ))
            .await;

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
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");

        let parts: Vec<&str> = file_name.splitn(2, '_').collect();
        let version = parts.first().unwrap_or(&"unknown").to_string();

        Ok(version)
    }

    /// 列出可用的备份
    pub async fn list_available_backups(
        &self,
        plugin_id: &str,
    ) -> PluginResult<Vec<crate::infrastructure::storage::backup::BackupInfo>> {
        self.deps
            .backup_manager
            .list_backups(plugin_id)
            .await
            .map_err(|e| PluginError::Rollback(format!("获取备份列表失败: {}", e)))
    }

    /// 删除备份
    pub async fn delete_backup(&self, backup_path: &PathBuf) -> PluginResult<()> {
        self.deps
            .backup_manager
            .delete_backup(backup_path)
            .await
            .map_err(|e| PluginError::Rollback(format!("删除备份失败: {}", e)))
    }

    /// 清理旧备份
    pub async fn cleanup_old_backups(
        &self,
        plugin_id: &str,
        keep_count: usize,
    ) -> PluginResult<usize> {
        self.deps
            .backup_manager
            .cleanup_old_backups(plugin_id, keep_count)
            .await
            .map_err(|e| PluginError::Rollback(format!("清理旧备份失败: {}", e)))
    }
}

impl Default for RollbackService {
    fn default() -> Self {
        Self::new(RollbackServiceDeps {
            repository: Arc::new(PluginRepository::default()),
            cache: Arc::new(LayeredCacheManager::default()),
            storage: Arc::new(FileStorage::new(std::path::Path::new(""))),
            backup_manager: Arc::new(BackupManager::new(PathBuf::from("./backups"))),
            event_bus: Arc::new(EventBus::new()),
            audit_logger: Arc::new(AuditLogger::default()),
            contexts: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
        })
    }
}
