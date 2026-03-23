//! 回滚服务模块
//!
//! 处理插件回滚流程，提供将插件恢复到之前版本的功能。
//!
//! # 功能概述
//!
//! - 从备份恢复插件
//! - 自动选择最近备份
//! - 支持指定备份路径
//! - 管理备份生命周期
//!
//! # 回滚流程
//!
//! 1. 检查插件存在
//! 2. 获取备份信息
//! 3. 停用当前版本
//! 4. 恢复备份
//! 5. 更新数据库记录
//! 6. 记录审计日志

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
///
/// 包含插件回滚所需的所有参数。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackRequest {
    /// 插件ID
    ///
    /// 要回滚的插件的唯一标识符。
    pub plugin_id: String,

    /// 备份路径
    ///
    /// 指定要恢复的备份路径。
    /// 如果未指定，则使用最近的备份。
    pub backup_path: Option<PathBuf>,

    /// 是否自动激活
    ///
    /// 回滚完成后是否自动激活插件。
    /// 如果回滚前插件已激活，回滚后将自动激活。
    pub auto_activate: bool,
}

/// 回滚响应
///
/// 包含回滚操作的结果信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackResponse {
    /// 插件ID
    ///
    /// 被回滚的插件唯一标识符。
    pub plugin_id: String,

    /// 回滚前版本
    ///
    /// 回滚操作前的插件版本号。
    pub from_version: String,

    /// 回滚后版本
    ///
    /// 回滚操作后的插件版本号。
    pub to_version: String,

    /// 是否成功
    ///
    /// 指示回滚操作是否成功完成。
    pub success: bool,

    /// 消息
    ///
    /// 回滚结果的描述性消息。
    pub message: String,
}

/// 回滚服务依赖
///
/// 包含回滚服务运行所需的所有依赖项。
pub struct RollbackServiceDeps {
    /// 数据仓库
    ///
    /// 用于查询和更新插件信息。
    pub repository: Arc<PluginRepository>,

    /// 缓存管理器
    ///
    /// 用于清除插件缓存。
    pub cache: Arc<LayeredCacheManager>,

    /// 文件存储
    ///
    /// 用于执行文件系统操作。
    pub storage: Arc<FileStorage>,

    /// 备份管理器
    ///
    /// 用于管理插件备份，查找和恢复备份。
    pub backup_manager: Arc<BackupManager>,

    /// 事件总线
    ///
    /// 用于发布插件回滚事件。
    pub event_bus: Arc<EventBus>,

    /// 审计日志
    ///
    /// 用于记录回滚操作的审计日志。
    pub audit_logger: Arc<AuditLogger>,

    /// 插件上下文映射
    ///
    /// 存储插件的运行时上下文信息。
    pub contexts: Arc<tokio::sync::RwLock<std::collections::HashMap<String, PluginContext>>>,
}

/// 回滚服务
///
/// 提供插件回滚功能的核心服务。
///
/// # 示例
///
/// ```rust,no_run
/// use cmx_plugin::service::rollback::{RollbackService, RollbackRequest};
/// use std::path::PathBuf;
///
/// # async fn example(service: &RollbackService) -> Result<(), cmx_plugin::error::PluginError> {
/// // 使用最近的备份回滚
/// let request = RollbackRequest {
///     plugin_id: "my-plugin".to_string(),
///     backup_path: None,
///     auto_activate: true,
/// };
///
/// let response = service.rollback(request).await?;
/// println!("插件从 {} 回滚到 {}", response.from_version, response.to_version);
///
/// // 指定备份路径回滚
/// let request = RollbackRequest {
///     plugin_id: "my-plugin".to_string(),
///     backup_path: Some(PathBuf::from("./backups/my-plugin/1.0.0_20240101")),
///     auto_activate: false,
/// };
///
/// let response = service.rollback(request).await?;
/// # Ok(())
/// # }
/// ```
pub struct RollbackService {
    deps: RollbackServiceDeps,
}

impl RollbackService {
    /// 创建新的回滚服务
    ///
    /// # 参数
    ///
    /// * `deps` - 回滚服务的依赖项
    ///
    /// # 返回值
    ///
    /// 返回初始化后的回滚服务实例
    pub fn new(deps: RollbackServiceDeps) -> Self {
        Self { deps }
    }

    /// 回滚插件
    ///
    /// 执行完整的插件回滚流程。
    ///
    /// # 参数
    ///
    /// * `request` - 回滚请求，包含插件ID、备份路径等参数
    ///
    /// # 返回值
    ///
    /// 返回回滚响应，包含回滚前后版本等信息。
    ///
    /// # 错误
    ///
    /// - `PluginError::PluginNotFound`: 插件不存在
    /// - `PluginError::Rollback`: 没有可用的备份
    /// - `PluginError::Rollback`: 恢复备份失败
    ///
    /// # 流程说明
    ///
    /// 1. **检查插件存在**: 验证要回滚的插件是否已安装
    /// 2. **获取备份**: 获取指定备份或最近的备份
    /// 3. **停用插件**: 如果插件已激活，先停用
    /// 4. **删除当前文件**: 删除当前版本的文件
    /// 5. **恢复备份**: 从备份恢复插件
    /// 6. **更新记录**: 更新数据库中的插件信息
    /// 7. **清除缓存**: 清除插件相关的缓存
    /// 8. **更新上下文**: 更新运行时上下文
    /// 9. **记录审计**: 记录回滚操作日志
    /// 10. **发布事件**: 通知其他组件
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

        // 步骤4：删除当前版本文件
        let install_path = PathBuf::from(&plugin.install_path);

        if install_path.exists() {
            self.deps
                .storage
                .remove_dir(&install_path)
                .map_err(|e| PluginError::Rollback(format!("删除当前版本文件失败: {}", e)))?;
        }

        // 步骤5：恢复备份
        self.deps
            .backup_manager
            .restore_backup(&backup_path, &install_path)
            .await
            .map_err(|e| PluginError::Rollback(format!("恢复备份失败: {}", e)))?;

        // 步骤6：更新数据库记录
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

        // 步骤7：清除缓存
        self.deps
            .cache
            .delete(&format!("plugin:{}", request.plugin_id))
            .await;

        // 步骤8：更新上下文
        {
            let mut contexts = self.deps.contexts.write().await;
            if let Some(context) = contexts.get_mut(&request.plugin_id) {
                context.version = to_version.clone();
            }
        }

        // 步骤9: 记录审计日志
        let duration_ms = start_time.elapsed().as_millis() as i64;
        let audit_record = crate::audit::record::AuditRecord::success(
            request.plugin_id.clone(),
            crate::audit::record::OperationType::Rollback,
        )
        .with_details(serde_json::json!({
            "from_version": from_version,
            "to_version": to_version,
            "backup_path": backup_path.to_string_lossy().to_string(),
        }))
        .with_old_value(from_version.clone())
        .with_new_value(to_version.clone())
        .with_completed(duration_ms);
        self.deps.audit_logger.log(audit_record).await;

        // 步骤10：发布事件
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
    ///
    /// 备份路径格式通常为：`{version}_{timestamp}`
    ///
    /// # 参数
    ///
    /// * `path` - 备份文件路径
    ///
    /// # 返回值
    ///
    /// 返回解析出的版本号字符串。
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
    ///
    /// 获取指定插件的所有可用备份列表。
    ///
    /// # 参数
    ///
    /// * `plugin_id` - 插件 ID
    ///
    /// # 返回值
    ///
    /// 返回备份信息列表，按时间倒序排列（最新的在前）。
    ///
    /// # 错误
    ///
    /// - `PluginError::Rollback`: 获取备份列表失败
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
    ///
    /// 删除指定的备份文件。
    ///
    /// # 参数
    ///
    /// * `backup_path` - 要删除的备份路径
    ///
    /// # 返回值
    ///
    /// 成功时返回 `Ok(())`。
    ///
    /// # 错误
    ///
    /// - `PluginError::Rollback`: 删除备份失败
    pub async fn delete_backup(&self, backup_path: &PathBuf) -> PluginResult<()> {
        self.deps
            .backup_manager
            .delete_backup(backup_path)
            .await
            .map_err(|e| PluginError::Rollback(format!("删除备份失败: {}", e)))
    }

    /// 清理旧备份
    ///
    /// 保留指定数量的最新备份，删除其余备份。
    ///
    /// # 参数
    ///
    /// * `plugin_id` - 插件 ID
    /// * `keep_count` - 要保留的备份数量
    ///
    /// # 返回值
    ///
    /// 返回被删除的备份数量。
    ///
    /// # 错误
    ///
    /// - `PluginError::Rollback`: 清理备份失败
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
    /// 创建默认配置的回滚服务
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
