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
use crate::fetcher::local::LocalFetcher;
use crate::fetcher::remote::RemoteFetcher;

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
}

impl UpgradeService {
    /// 创建新的升级服务
    pub fn new(deps: UpgradeServiceDeps) -> Self {
        Self { deps }
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

        // 步骤1：检查插件存在
        let plugin = self
            .deps
            .repository
            .find_plugin(&request.plugin_id)
            .await?
            .ok_or_else(|| PluginError::plugin_not_found(&request.plugin_id))?;

        let old_version = plugin.version.clone();

        // 步骤2：获取新版本插件包
        let package_path = self
            .fetch_package(&request.source, request.version_constraint.as_deref())
            .await?;

        // 步骤3：准备临时目录
        let temp_dir = self
            .deps
            .temp_root
            .join(format!("plugin_upgrade_{}", uuid::Uuid::new_v4()));
        let (extract_path, needs_cleanup) = self
            .prepare_package_for_validation(&package_path, &temp_dir)
            .await?;

        let _cleanup = TempDirCleanup::new(needs_cleanup.then_some(temp_dir.clone()));

        // 步骤4：安全验证
        let validation_result = self
            .deps
            .security_validator
            .validate_plugin_package(&extract_path)
            .await;
        if !validation_result.passed {
            let errors = validation_result.errors.join(", ");
            return Err(PluginError::Security(format!("安全验证失败: {}", errors)));
        }

        // 步骤5：解析新版本插件定义
        let new_plugin_def = self.parse_plugin_definition(&extract_path).await?;
        let new_version = new_plugin_def
            .version
            .clone()
            .unwrap_or_else(|| "1.0.0".to_string());

        // 步骤6：验证版本升级
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

        // 步骤7：创建备份
        let install_path = PathBuf::from(&plugin.install_path);
        let backup_path = self
            .deps
            .backup_manager
            .create_backup(&request.plugin_id, &old_version, &install_path)
            .await
            .map_err(|e| PluginError::Upgrade(format!("创建备份失败: {}", e)))?;

        // 步骤8：停用插件（如果已激活）
        let was_activated = plugin.status == "activated";
        if was_activated {
            // 直接更新状态，不调用 deactivate（避免循环依赖）
            let fields = crate::infrastructure::database::repository::PluginUpdateFields {
                status: Some("installed".to_string()),
                ..Default::default()
            };
            self.deps.repository.update_plugin(&request.plugin_id, &fields).await?;
        }

        // 步骤9：删除旧文件
        if install_path.exists() {
            self.deps
                .storage
                .remove_dir(&install_path)
                .map_err(|e| PluginError::Upgrade(format!("删除旧文件失败: {}", e)))?;
        }

        // 步骤10：安装新版本
        self.deps.storage.create_dir(&install_path)?;
        self.copy_plugin_files(&extract_path, &install_path).await?;

        // 步骤11：更新数据库记录
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

        // 更新上下文
        {
            let mut contexts = self.deps.contexts.write().await;
            if let Some(context) = contexts.get_mut(&request.plugin_id) {
                context.version = new_version.clone();
            }
        }

        // 步骤12：记录审计日志
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

        // 发布事件
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

    /// 获取插件包
    async fn fetch_package(
        &self,
        source: &PluginSource,
        version_constraint: Option<&str>,
    ) -> PluginResult<PathBuf> {
        match source {
            PluginSource::Local { path } => {
                let fetcher = LocalFetcher::new(&self.deps.plugin_root);
                fetcher
                    .fetch(&crate::fetcher::source::PluginSource::local(path.clone()))
                    .await
                    .map_err(|e| PluginError::Upgrade(format!("获取本地插件包失败: {}", e)))
            }
            PluginSource::Remote { url, checksum } => {
                let fetcher = RemoteFetcher::new(self.deps.temp_root.clone());
                fetcher
                    .fetch(&crate::fetcher::source::PluginSource::remote(
                        url.clone(),
                        checksum.clone(),
                    ))
                    .await
                    .map_err(|e| PluginError::Upgrade(format!("获取远程插件包失败: {}", e)))
            }
            PluginSource::Registry {
                registry_url,
                package_name,
            } => {
                let registry_info =
                    crate::fetcher::registry::RegistryInfo::new(registry_url.clone().unwrap_or_default());
                let fetcher = crate::fetcher::registry::RegistryFetcher::new(
                    registry_info,
                    self.deps.temp_root.clone(),
                );

                fetcher
                    .fetch_by_name(package_name, version_constraint.map(|s| s.to_string()))
                    .await
                    .map_err(|e| PluginError::Upgrade(format!("从注册表获取插件包失败: {}", e)))
            }
        }
    }

    /// 准备插件包用于验证
    async fn prepare_package_for_validation(
        &self,
        package_path: &std::path::Path,
        temp_dir: &std::path::Path,
    ) -> PluginResult<(std::path::PathBuf, bool)> {
        let is_zip = package_path
            .extension()
            .map(|ext| ext == "zip")
            .unwrap_or(false);

        if is_zip {
            std::fs::create_dir_all(temp_dir)
                .map_err(|e| PluginError::Upgrade(format!("创建临时目录失败: {}", e)))?;

            self.extract_zip(package_path, temp_dir).await?;

            let extract_path = self.find_plugin_root_in_dir(temp_dir)?;

            tracing::info!("插件包已解压到临时目录: {}", extract_path.display());

            Ok((extract_path, true))
        } else if package_path.is_dir() {
            Ok((package_path.to_path_buf(), false))
        } else {
            Err(PluginError::Upgrade(format!(
                "不支持的插件包格式: {}",
                package_path.display()
            )))
        }
    }

    /// 在解压目录中查找插件根目录
    fn find_plugin_root_in_dir(&self, dir: &std::path::Path) -> PluginResult<std::path::PathBuf> {
        if dir.join("manifest.json").exists() {
            return Ok(dir.to_path_buf());
        }

        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if path.join("manifest.json").exists() {
                        return Ok(path);
                    }
                    if let Ok(found) = self.find_plugin_root_in_dir(&path) {
                        return Ok(found);
                    }
                }
            }
        }

        Ok(dir.to_path_buf())
    }

    /// 解析插件定义
    async fn parse_plugin_definition(
        &self,
        package_path: &std::path::Path,
    ) -> PluginResult<cmx_core::model::meta::plugin::PluginDefinition> {
        let manifest_path = package_path.join("manifest.json");

        if !manifest_path.exists() {
            return Err(PluginError::Metadata(
                "插件定义文件 manifest.json 不存在".to_string(),
            ));
        }

        let content = std::fs::read_to_string(&manifest_path)
            .map_err(|e| PluginError::Metadata(format!("读取插件定义文件失败: {}", e)))?;

        let manifest: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| PluginError::Metadata(format!("解析 manifest.json 失败: {}", e)))?;

        let plugin_value = manifest.get("plugin").ok_or_else(|| {
            PluginError::Metadata("manifest.json 缺少 plugin 对象".to_string())
        })?;

        let definition: cmx_core::model::meta::plugin::PluginDefinition =
            serde_json::from_value(plugin_value.clone())
                .map_err(|e| PluginError::Metadata(format!("解析 plugin 定义失败: {}", e)))?;

        Ok(definition)
    }

    /// 复制插件文件
    async fn copy_plugin_files(
        &self,
        source: &std::path::Path,
        target: &std::path::Path,
    ) -> PluginResult<()> {
        if source.is_dir() {
            self.deps
                .storage
                .copy_dir(source, target)
                .map_err(|e| PluginError::Upgrade(format!("复制插件文件失败: {}", e)))?;
        }
        Ok(())
    }

    /// 解压 ZIP 文件
    async fn extract_zip(
        &self,
        zip_path: &std::path::Path,
        target: &std::path::Path,
    ) -> PluginResult<()> {
        cmx_utils::zip::ZipExtractor::extract(zip_path, target)
            .map_err(|e| PluginError::Upgrade(format!("解压插件包失败: {}", e)))?;

        Ok(())
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
