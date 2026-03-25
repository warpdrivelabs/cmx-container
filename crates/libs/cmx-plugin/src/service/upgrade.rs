//! 升级服务模块
//!
//! 处理插件升级流程，提供完整的插件版本升级功能。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::audit::logger::AuditLogger;
use crate::common::{DefinitionUtils, PackageUtils, PackageUtilsDeps};
use crate::core::context::PluginContext;
use crate::core::registry::PluginRegistry;
use crate::domain::plugin::PluginSource;
use crate::error::{PluginError, PluginResult};
use crate::infrastructure::cache::layered::LayeredCacheManager;
use crate::infrastructure::database::deployment::DeploymentRepository;
use crate::infrastructure::database::repository::PluginRepository;
use crate::infrastructure::database::table_metadata::TableMetadataRepository;
use crate::infrastructure::database::version_history::VersionHistoryRepository;
use crate::infrastructure::messaging::event::{Event, EventBus, EventType};
use crate::infrastructure::storage::TempDirCleanup;
use crate::infrastructure::storage::backup::BackupManager;
use crate::infrastructure::storage::file::FileStorage;
use crate::security::validator::SecurityValidator;
use chrono::Utc;
use cmx_database::get_default_db_manager;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

/// 升级请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpgradeRequest {
    /// 插件ID
    pub plugin_id: String,
    /// 新版本来源
    pub source: PluginSource,
    /// 版本约束
    #[serde(default)]
    pub version_constraint: Option<String>,
    /// 是否强制升级（忽略版本检查）
    pub force: bool,
    /// 操作者
    pub operator: String,
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
    /// 部署仓库
    pub deployment_repository: Arc<DeploymentRepository>,
    /// 版本历史仓库
    pub version_history_repository: Arc<VersionHistoryRepository>,
    /// 表元数据仓库
    pub table_metadata_repository: Arc<TableMetadataRepository>,
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
    /// 插件上下文映射
    pub contexts: Arc<RwLock<std::collections::HashMap<String, PluginContext>>>,
    /// 安装根目录
    pub plugin_root: PathBuf,
    /// 临时目录
    pub temp_root: PathBuf,
    /// 默认数据库ID
    pub default_database_id: String,
    /// 节点ID
    pub node_id: String,
    /// 节点名称
    pub node_name: Option<String>,
    /// 节点类型
    pub node_type: Option<String>,
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
        Self {
            deps,
            package_utils,
        }
    }

    /// 执行升级操作
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

        // 步骤2: 检查节点部署记录
        let existing_deployment = self
            .deps
            .deployment_repository
            .find_deployment(&request.plugin_id, &self.deps.node_id, &old_version)
            .await?;

        if existing_deployment.is_none() {
            return Err(PluginError::invalid_state(
                &request.plugin_id,
                "not_deployed",
                "节点未部署此插件，请使用安装功能",
            ));
        }

        // 步骤3: 获取新版本插件包
        let package_path = self
            .package_utils
            .fetch_package(
                &request.source,
                request.version_constraint.as_deref(),
                "升级",
            )
            .await?;

        // 步骤4: 解压到临时目录
        let temp_dir = self
            .deps
            .temp_root
            .join(format!("plugin_upgrade_{}", uuid::Uuid::new_v4()));
        let (extract_path, needs_cleanup) =
            self.package_utils
                .prepare_package_for_validation(&package_path, &temp_dir, "升级")?;

        let _cleanup = TempDirCleanup::new(needs_cleanup.then_some(temp_dir.clone()));

        // 步骤5: 安全验证和元数据解析
        let validation_result = self
            .deps
            .security_validator
            .validate_plugin_package(&extract_path)
            .await;
        if !validation_result.passed {
            let errors = validation_result.errors.join(", ");
            return Err(PluginError::Security(format!("安全验证失败: {}", errors)));
        }

        let plugin_def = DefinitionUtils::parse_plugin_definition(&extract_path)?;
        let new_version = plugin_def
            .version
            .clone()
            .unwrap_or_else(|| "1.0.0".to_string());

        // 步骤6: 版本检查
        if !request.force {
            if new_version <= old_version {
                return Err(PluginError::Upgrade(format!(
                    "升级版本必须大于当前版本: 当前 {}, 新版本 {}",
                    old_version, new_version
                )));
            }
        }

        let plugin_id = request.plugin_id.clone();

        // 步骤7: 创建新版本目录 (plugin_id/new_version/)
        let install_path = self.deps.plugin_root.join(&plugin_id).join(&new_version);
        if install_path.exists() {
            self.deps.storage.remove_dir(&install_path)?;
        }
        self.deps.storage.create_dir(&install_path)?;

        // 步骤8: 复制文件到新版本目录
        self.package_utils
            .copy_plugin_files(&extract_path, &install_path, "升级")?;

        let db_id = plugin.db_id.clone();
        //开启事务
        let txn_guard = get_default_db_manager()
            .get_transaction_context()
            .begin_with_guard(db_id.clone().as_str())
            .await
            .map_err(|e| PluginError::Database(e.to_string()))?;

        // 步骤9: 创建数据库表
        crate::service::utils::create_plugin_tables(
            &db_id,
            &plugin_id,
            &new_version,
            &install_path,
            &plugin_def.table_config_files,
            None,
            Some(&self.deps.table_metadata_repository),
        )
        .await?;

        // 步骤10: 插入 cmx_plugin_versions 版本历史
        let db_record = crate::infrastructure::database::repository::PluginDbRecord {
            id: uuid::Uuid::new_v4().to_string(),
            plugin_id: plugin_id.clone(),
            name: plugin_def.name.clone(),
            version: new_version.clone(),
            wasm_path: install_path
                .join(&plugin_def.main_file)
                .to_string_lossy()
                .to_string(),
            install_path: install_path.to_string_lossy().to_string(),
            db_id: db_id.clone(),
            status: "installed".to_string(),
            is_system: false,
            is_locked: false,
            domain_code: plugin_def.domain_code.clone(),
            application_code: plugin_def.application_code.clone(),
            module_code: plugin_def.module_code.clone(),
            vendor_name: plugin_def.vendor_name.clone(),
            vendor_url: plugin_def.vendor_url.clone(),
            vendor_contact: plugin_def.vendor_contact.clone(),
            metadata: None,
            signature_algorithm: None,
            signer_key_id: None,
            create_time: Utc::now(),
            update_time: Utc::now(),
            archived: 0,
            create_by: None,
            create_name: None,
            update_by: None,
            update_name: None,
        };

        // 查询数据库是否已经有基线插件了
        let baseline_version = self
            .deps
            .repository
            .get_baseline_version(&plugin_id)
            .await?;

        let baseline_exist = baseline_version.is_some();

        let version_record =
            crate::infrastructure::database::version_history::VersionHistoryRecord {
                id: uuid::Uuid::new_v4().to_string(),
                plugin_id: plugin_id.clone(),
                version: new_version.clone(),
                install_path: install_path.to_string_lossy().to_string(),
                wasm_path: install_path
                    .join(&plugin_def.main_file)
                    .to_string_lossy()
                    .to_string(),
                is_current: true,
                installed_at: Utc::now(),
                create_time: Utc::now(),
                update_time: Utc::now(),
                uninstalled_at: None,
                archived: 0,
                create_by: None,
                create_name: None,
                update_by: None,
                update_name: None,
            };

        //基线插件不存在
        if !baseline_exist {
            //插入数据
            self.deps
                .repository
                .insert_plugin(&db_record, Some(txn_guard.txn_id()))
                .await?;
            //插入版本历史
            self.deps
                .version_history_repository
                .insert_version(&version_record, None)
                .await?;
        } else {
            //基线版本存在，
            //// 将之前的基线版本标记为非当前
            self.deps
                .version_history_repository
                .mark_all_not_current(&plugin_id, Some(txn_guard.txn_id()))
                .await?;

            let update_fields = crate::infrastructure::database::repository::PluginUpdateFields {
                plugin_id: plugin_id.clone(),
                name: plugin_def.name.clone(),
                version: new_version.clone(),
                wasm_path: install_path
                    .join(&plugin_def.main_file)
                    .to_string_lossy()
                    .to_string(),
                install_path: install_path.to_string_lossy().to_string(),
                db_id: db_id.clone(),
                status: "installed".to_string(),
                is_system: false,
                is_locked: false,
                domain_code: plugin_def.domain_code.clone(),
                application_code: plugin_def.application_code.clone(),
                module_code: plugin_def.module_code.clone(),
                vendor_name: plugin_def.vendor_name.clone(),
                vendor_url: plugin_def.vendor_url.clone(),
                vendor_contact: plugin_def.vendor_contact.clone(),
                metadata: None,
                signature_algorithm: None,
                signer_key_id: None,
                update_time: Utc::now(),
                archived: 0,
                create_by: None,
                create_name: None,
                update_by: None,
                update_name: None,
            };
            self.deps
                .repository
                .update_plugin(&plugin_id, &update_fields, Some(txn_guard.txn_id()))
                .await?;

            //插入版本历史
            self.deps
                .version_history_repository
                .insert_version(&version_record, None)
                .await?;
        }

        // 步骤12: 更新 cmx_plugin_deployments 节点部署记录
        if let Some(deployment) = existing_deployment {
            let update_fields =
                crate::infrastructure::database::deployment::DeploymentUpdateFields {
                    version: new_version.clone(),
                    ..Default::default()
                };
            self.deps
                .deployment_repository
                .update_deployment(&deployment.id, &update_fields, None)
                .await?;
        }

        // 步骤13: 更新注册表
        {
            let mut registry = self.deps.registry.write().await;
            if let Some(info) = registry.get(&plugin_id) {
                let mut info = info.clone();
                info.version = new_version.clone();
                registry.register(info);
            }
        }

        // 步骤14: 更新缓存
        let plugin_info = crate::domain::plugin::PluginInfo {
            id: plugin_id.clone(),
            name: plugin_def.name.clone(),
            version: new_version.clone(),
            description: None,
            author: None,
            source: request.source.clone(),
            status: crate::domain::plugin::PluginStatus::Installed,
            installed_at: Some(Utc::now()),
            updated_at: Some(Utc::now()),
        };
        self.deps
            .cache
            .set(
                &plugin_id,
                crate::infrastructure::cache::layered::CacheValue::Json(
                    serde_json::to_value(&plugin_info).unwrap(),
                ),
                None,
            )
            .await;

        // 步骤15: 记录审计日志
        let duration_ms = start_time.elapsed().as_millis() as i64;
        let audit_record = crate::audit::record::AuditRecord::success(
            plugin_id.clone(),
            crate::audit::record::OperationType::Upgrade,
        )
        .with_details(serde_json::json!({
            "old_version": old_version,
            "new_version": new_version,
            "node_id": self.deps.node_id,
        }))
        .with_old_value(old_version.clone())
        .with_new_value(new_version.clone())
        .with_completed(duration_ms);
        let _ = self.deps.audit_logger.log(audit_record).await;

        // 步骤16: 发布升级完成事件
        self.deps
            .event_bus
            .publish(Event::new(
                EventType::PluginUpgraded,
                plugin_id.clone(),
                serde_json::json!({
                    "old_version": old_version,
                    "new_version": new_version,
                    "node_id": self.deps.node_id,
                }),
            ))
            .await;
        //提交事务
        txn_guard
            .commit()
            .await
            .map_err(|e| PluginError::Database(e.to_string()))?;
        Ok(UpgradeResponse {
            plugin_id,
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
            deployment_repository: Arc::new(DeploymentRepository::default()),
            version_history_repository: Arc::new(VersionHistoryRepository::default()),
            table_metadata_repository: Arc::new(TableMetadataRepository::default()),
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
            default_database_id: "default".to_string(),
            node_id: "default".to_string(),
            node_name: None,
            node_type: None,
        })
    }
}
