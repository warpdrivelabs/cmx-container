//! 升级服务模块
//!
//! 处理插件升级流程，提供完整的插件版本升级功能。
//!
//! # 功能概述
//!
//! - 从不同来源获取新版本插件包
//! - 安全验证新版本
//! - 版本升级检查
//! - 自动备份旧版本
//! - 无缝升级（保持激活状态）
//!
//! # 升级流程
//!
//! 1. 检查插件存在
//! 2. 获取新版本插件包
//! 3. 解压到临时目录
//! 4. 安全验证和元数据解析
//! 5. 验证版本升级
//! 6. 创建备份
//! 7. 停用插件
//! 8. 删除旧文件
//! 9. 安装新版本
//! 10. 更新数据库记录
//! 11. 重新激活
//! 12. 记录审计日志

use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::error::{PluginError, PluginResult};
use crate::domain::plugin::PluginSource;
use crate::infrastructure::database::repository::PluginRepository;
use crate::infrastructure::cache::layered::LayeredCacheManager;
use crate::infrastructure::storage::file::FileStorage;
use crate::infrastructure::storage::backup::BackupManager;
use crate::infrastructure::storage::TempDirCleanup;
use crate::infrastructure::messaging::event::{EventBus, Event, EventType};
use crate::security::validator::SecurityValidator;
use crate::audit::logger::AuditLogger;
use crate::core::registry::PluginRegistry;
use crate::core::context::PluginContext;
use crate::common::{PackageUtils, DefinitionUtils, PackageUtilsDeps};

/// 升级请求
///
/// 包含插件升级所需的所有参数。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpgradeRequest {
    /// 插件ID
    ///
    /// 要升级的插件的唯一标识符。
    pub plugin_id: String,

    /// 新版本来源
    ///
    /// 支持三种来源类型：
    /// - `Local { path }`: 本地文件路径
    /// - `Remote { url, checksum }`: 远程 URL
    /// - `Registry { registry_url, package_name }`: 插件注册表
    pub source: PluginSource,

    /// 是否强制升级
    ///
    /// - `true`: 忽略版本检查，允许降级或相同版本
    /// - `false`: 只允许升级到更高版本
    pub force: bool,

    /// 是否自动激活
    ///
    /// 升级完成后是否自动激活插件。
    /// 如果升级前插件已激活，升级后将自动激活。
    pub auto_activate: bool,

    /// 是否保留旧版本备份
    ///
    /// - `true`: 升级前创建备份，可用于回滚
    /// - `false`: 不创建备份
    pub keep_backup: bool,

    /// 版本约束
    ///
    /// 仅对注册表来源有效。
    /// 支持语义化版本约束，如 "^1.0.0"、">=2.0.0"。
    #[serde(default)]
    pub version_constraint: Option<String>,
}

/// 升级响应
///
/// 包含升级操作的结果信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpgradeResponse {
    /// 插件ID
    ///
    /// 被升级的插件唯一标识符。
    pub plugin_id: String,

    /// 旧版本
    ///
    /// 升级前的插件版本号。
    pub old_version: String,

    /// 新版本
    ///
    /// 升级后的插件版本号。
    pub new_version: String,

    /// 是否成功
    ///
    /// 指示升级操作是否成功完成。
    pub success: bool,

    /// 消息
    ///
    /// 升级结果的描述性消息。
    pub message: String,
}

/// 升级服务依赖
///
/// 包含升级服务运行所需的所有依赖项。
pub struct UpgradeServiceDeps {
    /// 数据仓库
    ///
    /// 用于查询和更新插件信息。
    pub repository: Arc<PluginRepository>,

    /// 缓存管理器
    ///
    /// 用于缓存插件信息。
    pub cache: Arc<LayeredCacheManager>,

    /// 文件存储
    ///
    /// 用于执行文件系统操作。
    pub storage: Arc<FileStorage>,

    /// 备份管理器
    ///
    /// 用于创建和管理插件备份。
    pub backup_manager: Arc<BackupManager>,

    /// 安全验证器
    ///
    /// 用于验证新版本插件包的安全性。
    pub security_validator: Arc<SecurityValidator>,

    /// 事件总线
    ///
    /// 用于发布插件升级事件。
    pub event_bus: Arc<EventBus>,

    /// 审计日志
    ///
    /// 用于记录升级操作的审计日志。
    pub audit_logger: Arc<AuditLogger>,

    /// 插件注册表
    ///
    /// 用于在内存中管理已注册的插件。
    pub registry: Arc<RwLock<PluginRegistry>>,

    /// 插件上下文映射
    ///
    /// 存储插件的运行时上下文信息。
    pub contexts: Arc<RwLock<std::collections::HashMap<String, PluginContext>>>,

    /// 安装根目录
    ///
    /// 所有插件的安装根路径。
    pub plugin_root: PathBuf,

    /// 临时目录
    ///
    /// 用于存储临时文件。
    pub temp_root: PathBuf,
}

/// 升级服务
///
/// 提供插件升级功能的核心服务。
///
/// # 示例
///
/// ```rust,no_run
/// use cmx_plugin::service::upgrade::{UpgradeService, UpgradeRequest};
/// use cmx_plugin::domain::plugin::PluginSource;
/// use std::path::PathBuf;
///
/// # async fn example(service: &UpgradeService) -> Result<(), cmx_plugin::error::PluginError> {
/// let request = UpgradeRequest {
///     plugin_id: "my-plugin".to_string(),
///     source: PluginSource::Local {
///         path: PathBuf::from("./my-plugin-v2.zip"),
///     },
///     force: false,
///     auto_activate: true,
///     keep_backup: true,
///     version_constraint: None,
/// };
///
/// let response = service.upgrade(request).await?;
/// println!("插件从 {} 升级到 {}", response.old_version, response.new_version);
/// # Ok(())
/// # }
/// ```
pub struct UpgradeService {
    deps: UpgradeServiceDeps,
    package_utils: PackageUtils,
}

impl UpgradeService {
    /// 创建新的升级服务
    ///
    /// # 参数
    ///
    /// * `deps` - 升级服务的依赖项
    ///
    /// # 返回值
    ///
    /// 返回初始化后的升级服务实例
    pub fn new(deps: UpgradeServiceDeps) -> Self {
        let package_utils = PackageUtils::new(PackageUtilsDeps {
            plugin_root: deps.plugin_root.clone(),
            temp_root: deps.temp_root.clone(),
            storage: Some(deps.storage.clone()),
        });
        Self { deps, package_utils }
    }

    /// 执行升级操作
    ///
    /// 执行完整的插件升级流程。
    ///
    /// # 参数
    ///
    /// * `request` - 升级请求，包含插件ID、新版本来源等参数
    ///
    /// # 返回值
    ///
    /// 返回升级响应，包含旧版本、新版本等信息。
    ///
    /// # 错误
    ///
    /// - `PluginError::PluginNotFound`: 插件不存在
    /// - `PluginError::Upgrade`: 版本检查失败（非强制模式）
    /// - `PluginError::Security`: 安全验证失败
    /// - `PluginError::Upgrade`: 创建备份失败
    /// - `PluginError::Upgrade`: 文件操作失败
    ///
    /// # 流程说明
    ///
    /// 1. **检查插件存在**: 验证要升级的插件是否已安装
    /// 2. **获取新版本**: 从指定来源获取新版本插件包
    /// 3. **准备验证环境**: 解压 ZIP 到临时目录
    /// 4. **安全验证**: 验证新版本的安全性
    /// 5. **解析定义**: 读取新版本的元数据
    /// 6. **版本检查**: 验证新版本号大于当前版本（非强制模式）
    /// 7. **创建备份**: 备份当前版本文件
    /// 8. **停用插件**: 如果插件已激活，先停用
    /// 9. **删除旧文件**: 删除当前版本的文件
    /// 10. **安装新版本**: 复制新版本文件到安装目录
    /// 11. **更新记录**: 更新数据库中的插件信息
    /// 12. **重新激活**: 如果之前是激活状态，重新激活
    /// 13. **记录审计**: 记录升级操作日志
    /// 14. **发布事件**: 通知其他组件
    ///
    /// # 示例
    ///
    /// ```rust,no_run
    /// # use cmx_plugin::service::upgrade::{UpgradeService, UpgradeRequest};
    /// # use cmx_plugin::domain::plugin::PluginSource;
    /// # async fn example(service: &UpgradeService) -> Result<(), cmx_plugin::error::PluginError> {
    /// let request = UpgradeRequest {
    ///     plugin_id: "my-plugin".to_string(),
    ///     source: PluginSource::Remote {
    ///         url: "https://example.com/plugin-v2.zip".to_string(),
    ///         checksum: Some("sha256:abc123".to_string()),
    ///     },
    ///     force: false,
    ///     auto_activate: true,
    ///     keep_backup: true,
    ///     version_constraint: None,
    /// };
    ///
    /// let response = service.upgrade(request).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn upgrade(&self, request: UpgradeRequest) -> PluginResult<UpgradeResponse> {
        let start_time = std::time::Instant::now();

        // 步骤1: 检查插件存在
        let plugin = self
            .deps
            .repository
            .find_plugin(&request.plugin_id)
            .await?
            .ok_or_else(|| PluginError::plugin_not_found(&request.plugin_id))?;

        let old_version = plugin.version.clone();

        // 步骤2: 获取新版本插件包
        let package_path = self
            .package_utils
            .fetch_package(&request.source, request.version_constraint.as_deref(), "升级")
            .await?;

        // 步骤3: 解压到临时目录
        let temp_dir = self
            .deps
            .temp_root
            .join(format!("plugin_upgrade_{}", uuid::Uuid::new_v4()));
        let (extract_path, needs_cleanup) = self
            .package_utils
            .prepare_package_for_validation(&package_path, &temp_dir, "升级")?;

        let _cleanup = TempDirCleanup::new(needs_cleanup.then_some(temp_dir.clone()));

        // 步骤4: 安全验证和元数据解析
        let validation_result = self
            .deps
            .security_validator
            .validate_plugin_package(&extract_path)
            .await;
        if !validation_result.passed {
            let errors = validation_result.errors.join(", ");
            return Err(PluginError::Security(format!("安全验证失败: {}", errors)));
        }

        let new_plugin_def = DefinitionUtils::parse_plugin_definition(&extract_path)?;
        let new_version = new_plugin_def
            .version
            .clone()
            .unwrap_or_else(|| "1.0.0".to_string());

        // 步骤5: 验证版本升级
        if !request.force {
            let old_semver = crate::domain::version::SemanticVersion::parse(&old_version)
                .map_err(|e| PluginError::Upgrade(format!("解析旧版本失败: {}", e)))?;
            let new_semver = crate::domain::version::SemanticVersion::parse(&new_version)
                .map_err(|e| PluginError::Upgrade(format!("解析新版本失败: {}", e)))?;

            if new_semver <= old_semver {
                return Err(PluginError::Upgrade(format!(
                    "升级版本必须大于当前版本: 当前 {}, 新版本 {}",
                    old_version, new_version
                )));
            }
        }

        // 步骤6: 创建备份
        let install_path = PathBuf::from(&plugin.install_path);
        let backup_path = self
            .deps
            .backup_manager
            .create_backup(&request.plugin_id, &old_version, &install_path)
            .await
            .map_err(|e| PluginError::Upgrade(format!("创建备份失败: {}", e)))?;

        // 步骤7: 停用插件
        let was_activated = plugin.status == "activated";
        if was_activated {
            let fields = crate::infrastructure::database::repository::PluginUpdateFields {
                status: Some("installed".to_string()),
                ..Default::default()
            };
            self.deps.repository.update_plugin(&request.plugin_id, &fields).await?;
        }

        // 步骤8: 删除旧文件
        if install_path.exists() {
            self.deps
                .storage
                .remove_dir(&install_path)
                .map_err(|e| PluginError::Upgrade(format!("删除旧文件失败: {}", e)))?;
        }

        // 步骤9: 安装新版本
        self.deps.storage.create_dir(&install_path)?;
        self.package_utils.copy_plugin_files(&extract_path, &install_path, "升级")?;

        // 步骤10: 更新数据库记录
        let fields = crate::infrastructure::database::repository::PluginUpdateFields {
            version: Some(new_version.clone()),
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

        // 步骤11: 更新上下文
        {
            let mut contexts = self.deps.contexts.write().await;
            if let Some(context) = contexts.get_mut(&request.plugin_id) {
                context.version = new_version.clone();
            }
        }

        // 步骤12: 记录审计日志
        let duration_ms = start_time.elapsed().as_millis() as i64;
        let audit_record = crate::audit::record::AuditRecord::success(
            request.plugin_id.clone(),
            crate::audit::record::OperationType::Upgrade,
        )
        .with_details(serde_json::json!({
            "old_version": old_version,
            "new_version": new_version,
            "backup_path": backup_path.to_string_lossy().to_string(),
        }))
        .with_old_value(old_version.clone())
        .with_new_value(new_version.clone())
        .with_completed(duration_ms);
        self.deps.audit_logger.log(audit_record).await;

        // 步骤13: 发布升级完成事件
        self.deps
            .event_bus
            .publish(Event::new(
                EventType::PluginUpgraded,
                request.plugin_id.clone(),
                serde_json::json!({
                    "old_version": old_version,
                    "new_version": new_version,
                }),
            ))
            .await;

        Ok(UpgradeResponse {
            plugin_id: request.plugin_id,
            old_version,
            new_version,
            success: true,
            message: "插件升级成功".to_string(),
        })
    }
}

impl Default for UpgradeService {
    /// 创建默认配置的升级服务
    fn default() -> Self {
        use std::sync::Arc;

        Self::new(UpgradeServiceDeps {
            repository: Arc::new(PluginRepository::default()),
            cache: Arc::new(LayeredCacheManager::default()),
            storage: Arc::new(FileStorage::new(Path::new(""))),
            backup_manager: Arc::new(BackupManager::new(PathBuf::from("./backups"))),
            security_validator: Arc::new(SecurityValidator::new()),
            event_bus: Arc::new(EventBus::new()),
            audit_logger: Arc::new(AuditLogger::default()),
            registry: Arc::new(RwLock::new(PluginRegistry::new())),
            contexts: Arc::new(RwLock::new(std::collections::HashMap::new())),
            plugin_root: PathBuf::from("./plugins"),
            temp_root: PathBuf::from("./temp"),
        })
    }
}
