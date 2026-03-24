//! 安装服务模块
//!
//! 处理插件安装流程

use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::Utc;
use log::info;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use cmx_core::model::meta::base::TableDefineDbExecutor;
use cmx_database::get_default_db_manager;
use cmx_metadata::config::{load_table_defines_config_from_path, TableDefinesConfigManager};
use cmx_utils::ConfigManager;
use crate::error::{PluginError, PluginResult};
use crate::domain::plugin::{PluginInfo, PluginSource, PluginStatus};
use crate::infrastructure::database::repository::PluginRepository;
use crate::infrastructure::database::deployment::DeploymentRepository;
use crate::infrastructure::database::version_history::VersionHistoryRepository;
use crate::infrastructure::cache::layered::LayeredCacheManager;
use crate::infrastructure::storage::file::FileStorage;
use crate::infrastructure::storage::backup::BackupManager;
use crate::infrastructure::storage::TempDirCleanup;
use crate::infrastructure::messaging::event::{EventBus, Event, EventType};
use crate::security::validator::SecurityValidator;
use crate::audit::logger::AuditLogger;
use crate::core::registry::PluginRegistry;
use crate::core::context::PluginContext;
use crate::common::{PackageUtils, DefinitionUtils, DependencyUtils, PackageUtilsDeps, DependencyUtilsDeps};

/// 安装请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallRequest {
    /// 插件来源
    pub source: PluginSource,
    /// 目标数据库ID（可选）
    pub db_id: Option<String>,
    // /// 是否强制安装（覆盖已存在的插件）
    // pub force: bool,
    /// 是否自动激活
    pub auto_activate: bool,
    /// 版本约束（仅对注册表来源有效，如 "^1.0.0", ">=2.0.0"）
    #[serde(default)]
    pub version_constraint: Option<String>,
}

/// 安装响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallResponse {
    /// 插件ID
    pub plugin_id: String,
    /// 安装路径
    pub install_path: PathBuf,
    /// 是否成功
    pub success: bool,
    /// 消息
    pub message: String,
}

/// 安装服务依赖
pub struct InstallServiceDeps {
    /// 数据仓库
    pub repository: Arc<PluginRepository>,
    /// 部署仓库
    pub deployment_repository: Arc<DeploymentRepository>,
    /// 版本历史仓库
    pub version_history_repository: Arc<VersionHistoryRepository>,
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
    /// 默认数据库ID
    pub default_database_id: String,
    /// 节点ID
    pub node_id: String,
    /// 节点名称
    pub node_name: Option<String>,
    /// 节点类型
    pub node_type: Option<String>,
}

/// 安装服务
pub struct InstallService {
    deps: InstallServiceDeps,
    package_utils: PackageUtils,
    dependency_utils: DependencyUtils,
}

impl InstallService {
    /// 创建新的安装服务
    pub fn new(deps: InstallServiceDeps) -> Self {
        let package_utils = PackageUtils::new(PackageUtilsDeps {
            plugin_root: deps.plugin_root.clone(),
            temp_root: deps.temp_root.clone(),
            storage: Some(deps.storage.clone()),
        });
        let dependency_utils = DependencyUtils::new(DependencyUtilsDeps {
            repository: deps.repository.clone(),
        });
        Self { deps, package_utils, dependency_utils }
    }

    /// 执行安装操作
    ///
    /// 完整流程：
    /// 1. 获取插件包（zip 或文件夹）
    /// 2. 如果是 zip，解压到临时目录
    /// 3. 在临时目录进行安全验证和元数据解析
    /// 4. 检查已安装状态
    /// 5. 检查依赖
    /// 6. 创建安装目录
    /// 7. 复制文件到安装目录
    /// 8. 创建数据库表
    /// 9. 注册插件
    /// 10. 保存数据库记录
    /// 11. 更新缓存
    /// 12. 记录审计日志
    /// 13. 清理临时目录
    pub async fn install(&self, request: InstallRequest) -> PluginResult<InstallResponse> {
        let start_time = std::time::Instant::now();

        // 步骤1: 获取插件包（zip 或文件夹）
        let package_path = self
            .package_utils
            .fetch_package(&request.source, request.version_constraint.as_deref(), "安装")
            .await?;

        // 步骤2: 如果是 zip，解压到临时目录
        let temp_dir = self
            .deps
            .temp_root
            .join(format!("plugin_install_{}", uuid::Uuid::new_v4()));
        let (extract_path, needs_cleanup) = self
            .package_utils
            .prepare_package_for_validation(&package_path, &temp_dir, "安装")?;

        let _cleanup = TempDirCleanup::new(needs_cleanup.then_some(temp_dir.clone()));

        // 步骤3: 在临时目录进行安全验证和元数据解析
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
        let plugin_id = plugin_def.id.clone();
        let version = plugin_def
            .version
            .clone()
            .unwrap_or_else(|| "1.0.0".to_string());

        // 步骤4: 检查当前节点此插件版本是否已安装
        let existing_deployment = self.deps.deployment_repository
            .find_deployment(&plugin_id, &self.deps.node_id, &version)
            .await?;

        if existing_deployment.is_some() {
            return Err(PluginError::plugin_already_exists(&plugin_id));
        }


        // 步骤5: 检查依赖
        let registry = self.deps.registry.clone();
        let repository = self.deps.repository.clone();
        let dep_result = self.dependency_utils.check_plugin_dependencies(&plugin_def, |plugin_id| {
            let registry = registry.clone();
            let repository = repository.clone();
            let plugin_id = plugin_id.to_string();
            async move {
                {
                    let registry = registry.read().await;
                    if let Some(info) = registry.get(&plugin_id) {
                        return Ok(Some(info.clone()));
                    }
                }
                if let Some(record) = repository.find_plugin(&plugin_id).await? {
                    let info = PluginInfo {
                        id: record.plugin_id,
                        name: record.name,
                        version: record.version,
                        description: None,
                        author: record.vendor_name,
                        source: PluginSource::Local {
                            path: PathBuf::from(&record.install_path),
                        },
                        status: PluginStatus::Installed,
                        installed_at: Some(record.create_time),
                        updated_at: Some(record.update_time),
                    };
                    return Ok(Some(info));
                }
                Ok(None)
            }
        }).await?;
        if !dep_result.satisfied {
            let missing: Vec<String> = dep_result
                .missing
                .iter()
                .map(|m| format!("{} ({})", m.plugin_id, m.required_by))
                .collect();
            return Err(PluginError::Dependency(format!(
                "缺少依赖插件: {}",
                missing.join(", ")
            )));
        }

        // 步骤6: 创建安装目录 (plugin_id/version/)
        let install_path = self.deps.plugin_root.join(&plugin_id).join(&version);
        if install_path.exists() {
            self.deps.storage.remove_dir(&install_path)?;
        }
        self.deps.storage.create_dir(&install_path)?;

        // 步骤7: 复制文件到安装目录
        self.package_utils.copy_plugin_files(&extract_path, &install_path, "安装")?;


        // 步骤8: 创建插件数据库表
        let db_id = request
            .db_id
            .clone()
            .unwrap_or_else(|| self.deps.default_database_id.clone());

        //开启事务
        let txn_guard = get_default_db_manager()
            .get_transaction_context()
            .begin_with_guard(db_id.clone().as_str()).await
            .map_err(|e| PluginError::Database(e.to_string()))?;


        if !plugin_def.table_config_files.is_empty() {
            ///PostgreSQL 的行为：
            // 一旦事务中任何语句失败，整个事务进入 “aborted” 状态
            // 此后所有新 SQL 都会被拒绝，并返回 25P02 错误
            // 必须显式执行 ROLLBACK 才能退出这个状态
            //所以ddl语句不要在事务中执行
            self.create_plugin_tables(&plugin_def, &db_id, None, &install_path)
                .await?;
        }

        // 步骤9: 注册插件
        let plugin_info = PluginInfo {
            id: plugin_id.clone(),
            name: plugin_def.name.clone(),
            version: version.clone(),
            description: plugin_def.description.clone(),
            author: plugin_def.vendor_name.clone(),
            source: request.source.clone(),
            status: PluginStatus::Installed,
            installed_at: Some(Utc::now()),
            updated_at: Some(Utc::now()),
        };

        // 步骤10: 保存数据库记录
        let db_record = crate::infrastructure::database::repository::PluginDbRecord {
            id: uuid::Uuid::new_v4().to_string(),
            plugin_id: plugin_id.clone(),
            name: plugin_def.name.clone(),
            version: version.clone(),
            wasm_path: install_path
                .join(&plugin_def.main_file)
                .to_string_lossy()
                .to_string(),
            install_path: install_path.to_string_lossy().to_string(),
            config_path: None,
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
            activated_at: None,
            create_time: Utc::now(),
            update_time: Utc::now(),
            archived: 0,
            create_by: None,
            create_name: None,
            update_by: None,
            update_name: None,
        };

        self.deps.repository.insert_plugin(&db_record,Some(txn_guard.txn_id())).await?;

        // 步骤10.1: 【新增】插入 cmx_plugin_versions 版本历史
        let baseline_version = self.deps.repository
            .get_baseline_version(&plugin_id)
            .await?;

        let version_type = if baseline_version.is_some() { "upgrade" } else { "initial" };

        let version_record = crate::infrastructure::database::version_history::VersionHistoryRecord {
            id: uuid::Uuid::new_v4().to_string(),
            plugin_id: plugin_id.clone(),
            version: version.clone(),
            version_type: version_type.to_string(),
            from_version: baseline_version.clone(),
            install_path: install_path.to_string_lossy().to_string(),
            wasm_path: install_path.join(&plugin_def.main_file).to_string_lossy().to_string(),
            backup_path: None,
            is_current: true,
            installed_at: Utc::now(),
            uninstalled_at: None,
            installed_by: None,
            install_reason: None,
            archived: 0,
            create_by: None,
            create_name: None,
            update_by: None,
            update_name: None,
        };
        self.deps.version_history_repository
            .insert_version(&version_record, None)
            .await?;

        // 将之前的基线版本标记为非当前
        if baseline_version.is_some() {
            self.deps.version_history_repository
                .mark_all_not_current(&plugin_id, None)
                .await?;
        }

        // 步骤10.2: 【新增】插入 cmx_plugin_deployments 节点部署记录
        let existing_deployment = self.deps.deployment_repository
            .find_deployment(&plugin_id, &self.deps.node_id, &version)
            .await?;

        let deployment_record = crate::infrastructure::database::deployment::DeploymentRecord {
            id: uuid::Uuid::new_v4().to_string(),
            plugin_id: plugin_id.clone(),
            node_id: self.deps.node_id.clone(),
            node_name: self.deps.node_name.clone(),
            node_type: self.deps.node_type.clone(),
            version: version.clone(),
            deployment_type: if existing_deployment.is_some() { version_type.to_string() } else { "initial".to_string() },
            status: "deployed".to_string(),
            progress: 100,
            error_message: None,
            error_details: None,
            sync_token: None,
            last_sync_at: Some(Utc::now()),
            deployed_at: Utc::now(),
            validated_at: None,
            archived: 0,
            create_by: None,
            create_name: None,
            update_by: None,
            update_name: None,
        };
        self.deps.deployment_repository
            .insert_deployment(&deployment_record, None)
            .await?;

        {
            let mut registry = self.deps.registry.write().await;
            registry.register(plugin_info.clone());
        }

        {
            let mut contexts = self.deps.contexts.write().await;
            let context = PluginContext::from_db_record(&db_record);
            contexts.insert(plugin_id.clone(), context);
        }

        // 步骤11: 更新缓存
        self.deps
            .cache
            .set(
                &format!("plugin:{}", plugin_id),
                crate::infrastructure::cache::layered::CacheValue::Json(
                    serde_json::to_value(&plugin_info).unwrap(),
                ),
                None,
            )
            .await;

        // 步骤12: 记录审计日志
        let duration_ms = start_time.elapsed().as_millis() as i64;
        let audit_record = crate::audit::record::AuditRecord::success(
            plugin_id.clone(),
            crate::audit::record::OperationType::Install,
        )
            .with_node_id(self.deps.node_id.clone())
        .with_details(serde_json::json!({
            "version": version,
            "install_path": install_path.to_string_lossy().to_string(),
            "node_id": self.deps.node_id,
            "version_type": version_type,
        }))
        .with_new_value(install_path.to_string_lossy().to_string())
        .with_completed(duration_ms);
        self.deps.audit_logger.log(audit_record).await;

        // 步骤13: 发布安装完成事件（临时目录由 TempDirCleanup 自动清理）
        self.deps
            .event_bus
            .publish(Event::new(
                EventType::PluginInstalled,
                plugin_id.clone(),
                serde_json::json!({
                    "version": version,
                    "install_path": install_path.to_string_lossy().to_string(),
                }),
            ))
            .await;

        //提交事务
        txn_guard.commit().await.map_err(|e| PluginError::Database(e.to_string()))?;
 info!("返回结果");
        Ok(InstallResponse {
            plugin_id,
            install_path,
            success: true,
            message: "插件安装成功".to_string(),
        })
    }



    /// 创建插件数据库表
    ///
    /// 使用 cmx-metadata 解析表定义并创建数据库表。
    async fn create_plugin_tables(
        &self,
        plugin_def: &cmx_core::model::meta::plugin::PluginDefinition,
        db_id: &str,
        txn_id: Option<String>,
        install_path: &std::path::Path,
    ) -> PluginResult<()> {
        if plugin_def.table_config_files.is_empty() {
            return Ok(());
        }

        let mut table_config_manager = TableDefinesConfigManager::new();
        let executor = cmx_metadata::PgTableDefineExecutor::new(db_id, txn_id);
        for table_config_file in &plugin_def.table_config_files {
            let config_path = install_path.join(table_config_file);
            let table_df = load_table_defines_config_from_path(&config_path)
                .map_err(|e| PluginError::Metadata(format!("加载表配置文件失败: {}", e)))?;
            table_config_manager.add_config(table_df);
        }

        let table_defs = table_config_manager.load_all_tables(install_path)
            .map_err(|e| PluginError::Metadata(format!("加载表定义失败: {}", e)))?;
        for table_def in table_defs {
            executor
                .create_or_upgrade_table(&table_def).await
                .map_err(|e|
                    PluginError::Metadata(format!("创建或升级表{}失败: {}", &table_def.table_name, e)))?;
        }

        Ok(())
    }
}

impl Default for InstallService {
    fn default() -> Self {
        use std::sync::Arc;

        Self::new(InstallServiceDeps {
            repository: Arc::new(PluginRepository::default()),
            deployment_repository: Arc::new(DeploymentRepository::default()),
            version_history_repository: Arc::new(VersionHistoryRepository::default()),
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
