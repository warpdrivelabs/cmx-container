//! 卸载服务模块
//!
//! 处理插件卸载流程，提供完整的插件卸载功能。
//!
//! # 功能概述
//!
//! - 检查依赖关系
//! - 停用已激活的插件
//! - 可选保留配置和数据
//! - 清理数据库记录
//! - 更新缓存和注册表
//!
//! # 卸载流程
//!
//! 1. 检查插件存在
//! 2. 检查依赖（非强制模式）
//! 3. 停用插件（如果已激活）
//! 4. 创建备份（如果保留数据）
//! 5. 删除文件
//! 6. 清理数据库记录
//! 7. 更新注册表和上下文
//! 8. 清除缓存
//! 9. 记录审计日志

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
///
/// 包含插件卸载所需的所有参数。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UninstallRequest {
    /// 插件ID
    ///
    /// 要卸载的插件的唯一标识符。
    pub plugin_id: String,

    /// 是否强制卸载
    ///
    /// - `true`: 忽略依赖检查，强制卸载
    /// - `false`: 如果有其他插件依赖此插件，则拒绝卸载
    ///
    /// # 注意
    ///
    /// 强制卸载可能导致依赖此插件的其他插件无法正常工作。
    pub force: bool,

    /// 是否保留配置
    ///
    /// - `true`: 保留插件的配置目录
    /// - `false`: 删除所有插件文件
    pub keep_config: bool,

    /// 是否保留数据
    ///
    /// - `true`: 在卸载前创建数据备份
    /// - `false`: 不创建备份，直接删除
    ///
    /// # 说明
    ///
    /// 如果设置为 `true`，系统会在删除文件前创建一个备份，
    /// 备份可用于后续恢复数据。
    pub keep_data: bool,
}

/// 卸载响应
///
/// 包含卸载操作的结果信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UninstallResponse {
    /// 插件ID
    ///
    /// 被卸载的插件唯一标识符。
    pub plugin_id: String,

    /// 是否成功
    ///
    /// 指示卸载操作是否成功完成。
    pub success: bool,

    /// 消息
    ///
    /// 卸载结果的描述性消息。
    pub message: String,
}

/// 卸载服务依赖
///
/// 包含卸载服务运行所需的所有依赖项。
pub struct UninstallServiceDeps {
    /// 数据仓库
    ///
    /// 用于查询和删除插件记录。
    pub repository: Arc<PluginRepository>,

    /// 缓存管理器
    ///
    /// 用于清除插件缓存。
    pub cache: Arc<LayeredCacheManager>,

    /// 文件存储
    ///
    /// 用于删除插件文件。
    pub storage: Arc<FileStorage>,

    /// 备份管理器
    ///
    /// 用于创建卸载前的数据备份。
    pub backup_manager: Arc<BackupManager>,

    /// 事件总线
    ///
    /// 用于发布插件卸载事件。
    pub event_bus: Arc<EventBus>,

    /// 审计日志
    ///
    /// 用于记录卸载操作的审计日志。
    pub audit_logger: Arc<AuditLogger>,

    /// 插件注册表
    ///
    /// 用于从内存中移除插件注册信息。
    pub registry: Arc<tokio::sync::RwLock<PluginRegistry>>,

    /// 插件上下文映射
    ///
    /// 用于移除插件的运行时上下文。
    pub contexts: Arc<tokio::sync::RwLock<std::collections::HashMap<String, PluginContext>>>,
}

/// 卸载服务
///
/// 提供插件卸载功能的核心服务。
///
/// # 示例
///
/// ```rust,no_run
/// use cmx_plugin::service::uninstall::{UninstallService, UninstallRequest};
///
/// # async fn example(service: &UninstallService) -> Result<(), cmx_plugin::error::PluginError> {
/// let request = UninstallRequest {
///     plugin_id: "my-plugin".to_string(),
///     force: false,
///     keep_config: false,
///     keep_data: true,
/// };
///
/// let response = service.uninstall(request).await?;
/// println!("卸载结果: {}", response.message);
/// # Ok(())
/// # }
/// ```
pub struct UninstallService {
    deps: UninstallServiceDeps,
    dependency_utils: DependencyUtils,
}

impl UninstallService {
    /// 创建新的卸载服务
    ///
    /// # 参数
    ///
    /// * `deps` - 卸载服务的依赖项
    ///
    /// # 返回值
    ///
    /// 返回初始化后的卸载服务实例
    pub fn new(deps: UninstallServiceDeps) -> Self {
        let dependency_utils = DependencyUtils::new(DependencyUtilsDeps {
            repository: deps.repository.clone(),
        });
        Self { deps, dependency_utils }
    }

    /// 卸载插件
    ///
    /// 执行完整的插件卸载流程。
    ///
    /// # 参数
    ///
    /// * `request` - 卸载请求，包含插件ID和选项
    ///
    /// # 返回值
    ///
    /// 返回卸载响应，包含操作结果信息。
    ///
    /// # 错误
    ///
    /// - `PluginError::PluginNotFound`: 插件不存在
    /// - `PluginError::Dependency`: 有其他插件依赖此插件（非强制模式）
    /// - `PluginError::Uninstall`: 文件操作失败
    ///
    /// # 流程说明
    ///
    /// 1. **检查插件存在**: 验证要卸载的插件是否已安装
    /// 2. **检查依赖**: 验证是否有其他插件依赖此插件（非强制模式）
    /// 3. **停用插件**: 如果插件已激活，先停用
    /// 4. **创建备份**: 如果 `keep_data` 为 true，创建数据备份
    /// 5. **删除文件**: 删除插件安装目录（除非 `keep_config` 为 true）
    /// 6. **删除记录**: 从数据库中删除插件记录
    /// 7. **更新注册表**: 从内存注册表中移除插件
    /// 8. **更新上下文**: 移除插件的运行时上下文
    /// 9. **清除缓存**: 清除插件相关的缓存
    /// 10. **记录审计**: 记录卸载操作日志
    /// 11. **发布事件**: 通知其他组件
    ///
    /// # 示例
    ///
    /// ```rust,no_run
    /// # use cmx_plugin::service::uninstall::{UninstallService, UninstallRequest};
    /// # async fn example(service: &UninstallService) -> Result<(), cmx_plugin::error::PluginError> {
    /// // 完全卸载（不保留任何内容）
    /// let request = UninstallRequest {
    ///     plugin_id: "my-plugin".to_string(),
    ///     force: false,
    ///     keep_config: false,
    ///     keep_data: false,
    /// };
    /// let response = service.uninstall(request).await?;
    ///
    /// // 保留数据的卸载
    /// let request = UninstallRequest {
    ///     plugin_id: "another-plugin".to_string(),
    ///     force: false,
    ///     keep_config: true,
    ///     keep_data: true,
    /// };
    /// let response = service.uninstall(request).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn uninstall(&self, request: UninstallRequest) -> PluginResult<UninstallResponse> {
        let start_time = std::time::Instant::now();

        // 步骤1: 检查插件存在
        let plugin = self
            .deps
            .repository
            .find_plugin(&request.plugin_id)
            .await?
            .ok_or_else(|| PluginError::plugin_not_found(&request.plugin_id))?;

        // 步骤2: 检查依赖（非强制模式）
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

        // 步骤3: 停用插件（如果已激活）
        if plugin.status == "activated" {
            self.deactivate_plugin(&request.plugin_id).await?;
        }

        // 步骤4: 创建备份（如果保留数据）
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

        // 步骤5: 删除文件
        let install_path = PathBuf::from(&plugin.install_path);
        if install_path.exists() && !request.keep_config {
            self.deps
                .storage
                .remove_dir(&install_path)
                .map_err(|e| PluginError::Uninstall(format!("删除插件文件失败: {}", e)))?;
        }

        // 步骤6: 清理数据库记录
        self.deps
            .repository
            .delete_plugin(&request.plugin_id)
            .await?;

        // 步骤7: 更新注册表
        {
            let mut registry = self.deps.registry.write().await;
            registry.unregister(&request.plugin_id);
        }

        // 步骤8: 更新上下文
        {
            let mut contexts = self.deps.contexts.write().await;
            contexts.remove(&request.plugin_id);
        }

        // 步骤9: 清除缓存
        self.deps
            .cache
            .delete(&format!("plugin:{}", request.plugin_id))
            .await;

        // 步骤10: 记录审计日志
        let duration_ms = start_time.elapsed().as_millis() as i64;
        let audit_record = crate::audit::record::AuditRecord::success(
            request.plugin_id.clone(),
            crate::audit::record::OperationType::Uninstall,
        )
        .with_details(serde_json::json!({
            "version": plugin.version,
            "keep_config": request.keep_config,
            "keep_data": request.keep_data,
        }))
        .with_old_value(plugin.version.clone())
        .with_completed(duration_ms);
        self.deps.audit_logger.log(audit_record).await;

        // 步骤11: 发布卸载事件
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
    ///
    /// 将插件状态更新为停用状态。
    ///
    /// # 参数
    ///
    /// * `plugin_id` - 要停用的插件 ID
    ///
    /// # 返回值
    ///
    /// 成功时返回 `Ok(())`。
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
    /// 创建默认配置的卸载服务
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
