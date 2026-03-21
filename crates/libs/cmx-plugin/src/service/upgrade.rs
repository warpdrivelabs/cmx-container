//! 升级服务模块
//!
//! 处理插件升级流程

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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpgradeRequest {
    /// 插件ID
    pub plugin_id: String,
    /// 新版本来源
    pub source: PluginSource,
    /// 是否强制升级（忽略版本检查）
    pub force: bool,
    /// 是否自动激活
    pub auto_activate: bool,
    /// 是否保留旧版本备份
    pub keep_backup: bool,
    /// 版本约束（仅对注册表来源有效）
    #[serde(default)]
    pub version_constraint: Option<String>,
}

/// 升级响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpgradeResponse {
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

/// 升级服务依赖
pub struct UpgradeServiceDeps {
    /// 数据仓库
    pub repository: Arc<PluginRepository>,
    /// 缓存管理器
    pub cache: Arc<LayeredCacheManager>,
    /// 文件存储
    pub storage: Arc<FileStorage>,
    /// 备份管理器
    pub backup_manager: Arc<BackupManager>,
    /// 安全验证器
    pub security_validator: Arc<SecurityValidator>,
    /// 事件总线
    pub event_bus: Arc<EventBus>,
    /// 审计日志
    pub audit_logger: Arc<AuditLogger>,
    /// 插件注册表
    pub registry: Arc<RwLock<PluginRegistry>>,
    /// 插件上下文
    pub contexts: Arc<RwLock<std::collections::HashMap<String, PluginContext>>>,
    /// 安装根目录
    pub plugin_root: PathBuf,
    /// 临时目录
    pub temp_root: PathBuf,
}

/// 升级服务
pub struct UpgradeService {
    deps: UpgradeServiceDeps,
    package_utils: PackageUtils,
}

impl UpgradeService {
    /// 创建新的升级服务
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
    /// 完整流程：
    /// 1. 检查插件存在
    /// 2. 获取新版本插件包
    /// 3. 解压到临时目录
    /// 4. 安全验证和元数据解析
    /// 5. 验证版本升级
    /// 6. 创建备份
    /// 7. 停用插件
    /// 8. 删除旧文件
    /// 9. 安装新版本
    /// 10. 更新数据库记录
    /// 11. 重新激活
    /// 12. 记录审计日志
    pub async fn upgrade(&self, request: UpgradeRequest) -> PluginResult<UpgradeResponse> {
        let start_time = std::time::Instant::now();

        let plugin = self
            .deps
            .repository
            .find_plugin(&request.plugin_id)
            .await?
            .ok_or_else(|| PluginError::plugin_not_found(&request.plugin_id))?;

        let old_version = plugin.version.clone();

        let package_path = self
            .package_utils
            .fetch_package(&request.source, request.version_constraint.as_deref(), "升级")
            .await?;

        let temp_dir = self
            .deps
            .temp_root
            .join(format!("plugin_upgrade_{}", uuid::Uuid::new_v4()));
        let (extract_path, needs_cleanup) = self
            .package_utils
            .prepare_package_for_validation(&package_path, &temp_dir, "升级")?;

        let _cleanup = TempDirCleanup::new(needs_cleanup.then_some(temp_dir.clone()));

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

        let install_path = PathBuf::from(&plugin.install_path);
        let backup_path = self
            .deps
            .backup_manager
            .create_backup(&request.plugin_id, &old_version, &install_path)
            .await
            .map_err(|e| PluginError::Upgrade(format!("创建备份失败: {}", e)))?;

        let was_activated = plugin.status == "activated";
        if was_activated {
            let fields = crate::infrastructure::database::repository::PluginUpdateFields {
                status: Some("installed".to_string()),
                ..Default::default()
            };
            self.deps.repository.update_plugin(&request.plugin_id, &fields).await?;
        }

        if install_path.exists() {
            self.deps
                .storage
                .remove_dir(&install_path)
                .map_err(|e| PluginError::Upgrade(format!("删除旧文件失败: {}", e)))?;
        }

        self.deps.storage.create_dir(&install_path)?;
        self.package_utils.copy_plugin_files(&extract_path, &install_path, "升级")?;

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

        {
            let mut contexts = self.deps.contexts.write().await;
            if let Some(context) = contexts.get_mut(&request.plugin_id) {
                context.version = new_version.clone();
            }
        }

        let audit_record = crate::audit::record::AuditRecord::success(
            request.plugin_id.clone(),
            crate::audit::record::OperationType::Upgrade,
        )
        .with_details(serde_json::json!({
            "old_version": old_version,
            "new_version": new_version,
            "backup_path": backup_path.to_string_lossy().to_string(),
            "duration_ms": start_time.elapsed().as_millis(),
        }));
        self.deps.audit_logger.log(audit_record).await;

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
