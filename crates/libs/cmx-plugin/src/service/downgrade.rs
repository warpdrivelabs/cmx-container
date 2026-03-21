//! 降级服务模块
//!
//! 处理插件降级流程，提供将插件回退到指定旧版本的功能。
//!
//! # 功能概述
//!
//! - 从备份中恢复指定旧版本
//! - 自动备份当前版本
//! - 保持激活状态
//! - 记录审计日志
//!
//! # 降级流程
//!
//! 1. 检查插件存在
//! 2. 查找目标版本备份
//! 3. 创建当前版本备份
//! 4. 停用插件
//! 5. 恢复旧版本
//! 6. 更新数据库记录
//! 7. 记录审计日志

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
///
/// 包含插件降级所需的所有参数。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DowngradeRequest {
    /// 插件ID
    ///
    /// 要降级的插件的唯一标识符。
    pub plugin_id: String,

    /// 目标版本
    ///
    /// 要降级到的目标版本号。必须存在该版本的备份。
    pub target_version: String,

    /// 是否强制降级
    ///
    /// - `true`: 忽略版本检查，允许升级或相同版本
    /// - `false`: 只允许降级到更低版本
    pub force: bool,

    /// 是否自动激活
    ///
    /// 降级完成后是否自动激活插件。
    /// 如果降级前插件已激活，降级后将自动激活。
    pub auto_activate: bool,

    /// 是否保留当前版本备份
    ///
    /// - `true`: 降级前创建当前版本的备份
    /// - `false`: 不创建备份
    pub keep_backup: bool,
}

/// 降级响应
///
/// 包含降级操作的结果信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DowngradeResponse {
    /// 插件ID
    ///
    /// 被降级的插件唯一标识符。
    pub plugin_id: String,

    /// 旧版本
    ///
    /// 降级前的插件版本号。
    pub old_version: String,

    /// 新版本
    ///
    /// 降级后的插件版本号（即目标版本）。
    pub new_version: String,

    /// 是否成功
    ///
    /// 指示降级操作是否成功完成。
    pub success: bool,

    /// 消息
    ///
    /// 降级结果的描述性消息。
    pub message: String,
}

/// 降级服务依赖
///
/// 包含降级服务运行所需的所有依赖项。
pub struct DowngradeServiceDeps {
    /// 数据仓库
    ///
    /// 用于查询和更新插件信息。
    pub repository: Arc<PluginRepository>,

    /// 文件存储
    ///
    /// 用于执行文件系统操作。
    pub storage: Arc<FileStorage>,

    /// 备份管理器
    ///
    /// 用于管理插件备份，查找和恢复目标版本。
    pub backup_manager: Arc<BackupManager>,

    /// 事件总线
    ///
    /// 用于发布插件降级事件。
    pub event_bus: Arc<EventBus>,

    /// 审计日志
    ///
    /// 用于记录降级操作的审计日志。
    pub audit_logger: Arc<AuditLogger>,

    /// 插件上下文映射
    ///
    /// 存储插件的运行时上下文信息。
    pub contexts: Arc<tokio::sync::RwLock<std::collections::HashMap<String, PluginContext>>>,
}

/// 降级服务
///
/// 提供插件降级功能的核心服务。
///
/// # 示例
///
/// ```rust,no_run
/// use cmx_plugin::service::downgrade::{DowngradeService, DowngradeRequest};
///
/// # async fn example(service: &DowngradeService) -> Result<(), cmx_plugin::error::PluginError> {
/// let request = DowngradeRequest {
///     plugin_id: "my-plugin".to_string(),
///     target_version: "1.0.0".to_string(),
///     force: false,
///     auto_activate: true,
///     keep_backup: true,
/// };
///
/// let response = service.downgrade(request).await?;
/// println!("插件从 {} 降级到 {}", response.old_version, response.new_version);
/// # Ok(())
/// # }
/// ```
pub struct DowngradeService {
    deps: DowngradeServiceDeps,
}

impl DowngradeService {
    /// 创建新的降级服务
    ///
    /// # 参数
    ///
    /// * `deps` - 降级服务的依赖项
    ///
    /// # 返回值
    ///
    /// 返回初始化后的降级服务实例
    pub fn new(deps: DowngradeServiceDeps) -> Self {
        Self { deps }
    }

    /// 降级插件
    ///
    /// 执行完整的插件降级流程。
    ///
    /// # 参数
    ///
    /// * `request` - 降级请求，包含插件ID、目标版本等参数
    ///
    /// # 返回值
    ///
    /// 返回降级响应，包含旧版本、新版本等信息。
    ///
    /// # 错误
    ///
    /// - `PluginError::PluginNotFound`: 插件不存在
    /// - `PluginError::Downgrade`: 未找到目标版本的备份
    /// - `PluginError::Downgrade`: 创建备份失败
    /// - `PluginError::Downgrade`: 恢复备份失败
    ///
    /// # 流程说明
    ///
    /// 1. **检查插件存在**: 验证要降级的插件是否已安装
    /// 2. **查找备份**: 查找目标版本的备份文件
    /// 3. **创建备份**: 备份当前版本（可选）
    /// 4. **停用插件**: 如果插件已激活，先停用
    /// 5. **删除当前文件**: 删除当前版本的文件
    /// 6. **恢复旧版本**: 从备份恢复目标版本
    /// 7. **更新记录**: 更新数据库中的插件信息
    /// 8. **更新上下文**: 更新运行时上下文
    /// 9. **记录审计**: 记录降级操作日志
    /// 10. **发布事件**: 通知其他组件
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

        // 步骤5：删除当前版本文件
        if install_path.exists() {
            self.deps
                .storage
                .remove_dir(&install_path)
                .map_err(|e| PluginError::Downgrade(format!("删除当前文件失败: {}", e)))?;
        }

        // 步骤6：恢复旧版本
        self.deps
            .backup_manager
            .restore_backup(&target_backup.path, &install_path)
            .await
            .map_err(|e| PluginError::Downgrade(format!("恢复备份失败: {}", e)))?;

        // 步骤7：更新数据库记录
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

        // 步骤8：更新上下文
        {
            let mut contexts = self.deps.contexts.write().await;
            if let Some(context) = contexts.get_mut(&request.plugin_id) {
                context.version = request.target_version.clone();
            }
        }

        // 步骤9：记录审计日志
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

        // 步骤10：发布事件
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
    /// 创建默认配置的降级服务
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
