//! 安装服务模块
//!
//! 处理插件安装流程

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::audit::logger::AuditLogger;
use crate::common::{
    DefinitionUtils, DependencyUtils, DependencyUtilsDeps, PackageUtils, PackageUtilsDeps,
};
use crate::core::context::PluginContext;
use crate::core::registry::PluginRegistry;
use crate::domain::plugin::{PluginInfo, PluginSource, PluginStatus};
use crate::error::{PluginError, PluginResult};
use crate::infrastructure::cache::layered::LayeredCacheManager;
use crate::infrastructure::database::deployment::DeploymentRepository;
use crate::infrastructure::database::repository::PluginRepository;
use crate::infrastructure::database::version_history::VersionHistoryRepository;
use crate::infrastructure::storage::backup::BackupManager;
use crate::infrastructure::storage::file::FileStorage;
use crate::infrastructure::storage::TempDirCleanup;
use crate::security::validator::SecurityValidator;
use chrono::Utc;
use cmx_traits::GlobalEventBus;
use cmx_database::get_default_db_manager;
use cmx_traits::{plugin_events, PluginLifecyclePayload};
use log::warn;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

/// 安装请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallRequest {
    /// 插件来源
    pub source: PluginSource,
    /// 目标数据库ID（可选）
    pub db_id: Option<String>,
    /// 是否自动激活
    pub auto_activate: bool,
    /// 版本约束（仅对注册表来源有效，如 "^1.0.0", ">=2.0.0"）
    #[serde(default)]
    pub version_constraint: Option<String>,
    /// 构建类型 debug release
    pub build_type: Option<String>,
}

/// 安装响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallResponse {
    /// 插件ID
    pub plugin_id: String,
    /// 安装路径
    pub install_path: PathBuf,
    /// 插件版本
    pub version: String,
    /// 是否成功
    pub success: bool,
    /// 消息
    pub message: String,
}

/// 安装服务依赖
#[derive(Clone)]
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
    /// 服务存储
    pub service_storage: Arc<dyn cmx_traits::ServiceStorage>,
}

/// 安装服务
#[derive(Clone)]
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
        Self {
            deps,
            package_utils,
            dependency_utils,
        }
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
    /// 9. 保存数据库记录
    /// 10. 注册插件到内存
    /// 11. 更新缓存
    /// 12. 记录审计日志
    /// 13. 解析并存储服务定义
    /// 14. 提交事务
    pub async fn install(&self, request: InstallRequest) -> PluginResult<InstallResponse> {
        let start_time = std::time::Instant::now();

        let build_type = request.build_type.unwrap_or("release".to_string());

        // 步骤1: 获取插件包（zip 或文件夹）
        let package_path = self
            .package_utils
            .fetch_package(
                &request.source,
                request.version_constraint.as_deref(),
                "安装",
            )
            .await?;

        // 步骤2: 如果是 zip，解压到临时目录
        let temp_dir = self
            .deps
            .temp_root
            .join(format!("plugin_install_{}", uuid::Uuid::new_v4()));
        let (extract_path, needs_cleanup) =
            self.package_utils
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
        let install_version = plugin_def
            .version
            .clone()
            .unwrap_or_else(|| "1.0.0".to_string());

        // 步骤4: 检查当前节点此插件版本是否已安装
        let existing_deployment = self
            .deps
            .deployment_repository
            .find_deployment(&plugin_id, &self.deps.node_id, &install_version)
            .await?;

        if let Some(_node_deploment) = existing_deployment {
            let registry = self.deps.registry.read().await;
            if let Some(info) = registry.get(&plugin_id) {
                if info.clone().version > install_version {
                    return Err(PluginError::Install(format!(
                        "插件 {} 已安装版本 {}，要降级到 {} 请使用降级功能",
                        plugin_id, info.version, install_version
                    )));
                }

                return Ok(InstallResponse {
                    plugin_id,
                    install_path: info.install_path.clone(),
                    success: true,
                    version: install_version,
                    message: "插件已安装".to_string(),
                });
            } else {
                warn!("插件 {} 已部署，但未注册到registry", plugin_id);
            }
            return Err(PluginError::plugin_already_exists(&plugin_id));
        }

        // 步骤5: 检查依赖
        let registry = self.deps.registry.clone();
        let repository = self.deps.repository.clone();
        let dep_result = self
            .dependency_utils
            .check_plugin_dependencies(&plugin_def, |plugin_id| {
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
                            description: record.description,
                            author: record.vendor_name,
                            source: PluginSource::Local {
                                path: PathBuf::from(&record.install_path),
                            },
                            status: PluginStatus::Installed,
                            installed_at: Some(record.create_time),
                            updated_at: Some(record.update_time),
                            install_path: PathBuf::from(&record.install_path),
                            domain_code: record.domain_code.unwrap_or_default(),
                            application_code: record.application_code.unwrap_or_default(),
                            module_code: record.module_code.unwrap_or_default(),
                            plugin_type: record.plugin_type.clone().unwrap_or_default(),
                            source_path: record.source_path.clone(),
                        };
                        return Ok(Some(info));
                    }
                    Ok(None)
                }
            })
            .await?;
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
        let install_path = self.deps.plugin_root.join(&plugin_id).join(&install_version);
        if install_path.exists() {
            self.deps.storage.remove_dir(&install_path)?;
        }
        self.deps.storage.create_dir(&install_path)?;

        // 步骤7: 复制文件到安装目录
        self.package_utils
            .copy_plugin_files(&extract_path, &install_path, "安装")?;

        // 步骤8: 创建插件数据库表
        let target_db_id = request
            .db_id
            .clone()
            .unwrap_or_else(|| self.deps.default_database_id.clone());

        let default_db_id = self.deps.default_database_id.clone();

        // 开启事务（事务是针对默认表开启的）
        let txn_guard = get_default_db_manager()
            .get_transaction_context()
            .begin_with_guard(default_db_id.clone().as_str())
            .await
            .map_err(|e| PluginError::Database(e.to_string()))?;

        if !plugin_def.table_config_files.is_empty() {
            // PostgreSQL 的行为：
            // 一旦事务中任何语句失败，整个事务进入 "aborted" 状态
            // 此后所有新 SQL 都会被拒绝，并返回 25P02 错误
            // 必须显式执行 ROLLBACK 才能退出这个状态
            // 所以 DDL 语句不要在事务中执行
            crate::service::utils::create_plugin_tables(
                &target_db_id,
                &plugin_id,
                &install_version,
                &install_path,
                &plugin_def,
                Some(txn_guard.txn_id()),
            )
            .await?;
        }

        // 步骤9: 保存数据库记录
        let (zip_source_type, zip_source_url) = extract_source_info(&request.source);
        let db_record = super::record_builder::build_plugin_create_params(
            &plugin_def,
            &install_version,
            &install_path,
            &target_db_id,
            zip_source_url.as_deref(),
            zip_source_type.as_deref(),
        );

        // 查询数据库是否已经有基线插件了
        let baseline_version = self
            .deps
            .repository
            .get_baseline_version(&plugin_id)
            .await?;

        if let Some(ref db_version) = baseline_version {
            // 版本小不能用安装，应该使用降级
            if install_version < *db_version {
                return Err(PluginError::Install(format!(
                    "插件 {} 已安装版本 {}，要降级到 {} 请使用降级功能",
                    plugin_id, db_version, install_version
                )));
            }
        }

        // 步骤9.1: Upsert 插件记录（cmx_plugin）
        self.deps
            .repository
            .upsert_plugin(&db_record, Some(txn_guard.txn_id()))
            .await?;

        // 步骤9.2: 插入 cmx_plugin_versions 版本历史记录
        let wasm_path = super::record_builder::build_wasm_path(&install_path, &plugin_def);
        let version_record = super::record_builder::build_version_create_params(
            &plugin_id,
            &install_version,
            &install_path.to_string_lossy(),
            &wasm_path,
            zip_source_url.as_deref(),
            zip_source_type.as_deref(),
            Some(&plugin_def),
            build_type.as_str(),
        );
        self.deps
            .version_history_repository
            .upsert_version(&version_record, Some(txn_guard.txn_id()))
            .await?;

        // 标记当前版本
        self.deps
            .version_history_repository
            .set_current_version(
                &plugin_id,
                &install_version,
                install_path.to_string_lossy().to_string().as_str(),
                wasm_path.as_str(),
                Some(txn_guard.txn_id()),
            )
            .await?;

        // 步骤9.3: 插入 cmx_plugin_deployments 节点部署记录
        let existing_deployment_for_version = self
            .deps
            .deployment_repository
            .find_deployment(&plugin_id, &self.deps.node_id, &install_version)
            .await?;

        if existing_deployment_for_version.is_none() {
            let deployment_record = super::record_builder::build_deployment_create_params(
                &plugin_id,
                &self.deps.node_id,
                self.deps.node_type.as_deref(),
                &install_version,
            );
            self.deps
                .deployment_repository
                .insert_deployment(&deployment_record, Some(txn_guard.txn_id()))
                .await?;
        }

        // 步骤10: 注册插件
        let plugin_info = PluginInfo {
            id: plugin_id.clone(),
            name: plugin_def.name.clone(),
            version: install_version.clone(),
            description: plugin_def.description.clone(),
            author: plugin_def.vendor_name.clone(),
            source: request.source.clone(),
            status: PluginStatus::Installed,
            installed_at: Some(Utc::now()),
            updated_at: Some(Utc::now()),
            install_path: install_path.clone(),
            domain_code: plugin_def.domain_code.clone().unwrap_or_default(),
            application_code: plugin_def.application_code.clone().unwrap_or_default(),
            module_code: plugin_def.module_code.clone().unwrap_or_default(),
            plugin_type: plugin_def.r#type.clone(),
            source_path: plugin_def.source_path.clone(),
        };
        {
            let mut registry = self.deps.registry.write().await;
            registry.register(plugin_info.clone());
        }

        {
            let mut contexts = self.deps.contexts.write().await;
            let context = PluginContext::from_db_record(&db_record.to_record());
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
            "version": install_version,
            "install_path": install_path.to_string_lossy().to_string(),
            "node_id": self.deps.node_id,
        }))
        .with_new_value(install_path.to_string_lossy().to_string())
        .with_completed(duration_ms);

        let _ = self.deps.audit_logger.log(audit_record).await?;

        // 步骤13: 发布安装完成事件
        let payload = PluginLifecyclePayload::new(&plugin_id, &install_version)
            .with_install_path(install_path.clone())
            .with_wasm_path(PathBuf::from(&wasm_path));



        // 步骤14: 解析并存储服务定义（使用事务保证一致性）
        let parsed_services = crate::service::service_parser::parse_and_save_services(
            &install_path,
            &plugin_id,
            &install_version,
            &self.deps.service_storage,
            Some(txn_guard.txn_id()),
        )
        .await?;

        if !parsed_services.is_empty() {
            tracing::info!(
                "插件 {} 安装时解析到 {} 个服务定义",
                plugin_id,
                parsed_services.len()
            );
        }

        // 提交事务
        txn_guard
            .commit()
            .await
            .map_err(|e| PluginError::Database(e.to_string()))?;

        //发布事件
        GlobalEventBus::get()
            .publish(plugin_events::INSTALLED, serde_json::to_value(&payload).unwrap())
            .await;
        Ok(InstallResponse {
            plugin_id,
            install_path,
            version: install_version,
            success: true,
            message: "插件安装成功".to_string(),
        })
    }
}

/// 从 PluginSource 解析来源类型和地址
fn extract_source_info(source: &PluginSource) -> (Option<String>, Option<String>) {
    match source {
        PluginSource::Local { path } => {
            (Some("local".to_string()), Some(path.to_string_lossy().to_string()))
        }
        PluginSource::Remote { url, .. } => {
            (Some("url".to_string()), Some(url.clone()))
        }
        PluginSource::Registry { registry_url, package_name } => {
            let url = registry_url.as_ref().map(|s| s.as_str()).unwrap_or(package_name);
            (Some("registry".to_string()), Some(url.to_string()))
        }
    }
}

impl Default for InstallService {
    fn default() -> Self {
        use std::sync::Arc;
        use cmx_service::ServiceStorageImpl;
        use cmx_database::get_default_db_manager;
        use cmx_service::ServiceRepository;

        let db_manager = get_default_db_manager();
        let default_database_id = "primary".to_string();
        let service_repository = Arc::new(ServiceRepository::new(db_manager.clone(),default_database_id));
        let service_storage: Arc<dyn cmx_traits::ServiceStorage> =
            Arc::new(ServiceStorageImpl::new(service_repository.clone()));

        Self::new(InstallServiceDeps {
            repository: Arc::new(PluginRepository::default()),
            deployment_repository: Arc::new(DeploymentRepository::default()),
            version_history_repository: Arc::new(VersionHistoryRepository::default()),
            cache: Arc::new(LayeredCacheManager::default()),
            storage: Arc::new(FileStorage::new(Path::new(""))),
            backup_manager: Arc::new(BackupManager::new(PathBuf::from("./backups"))),
            security_validator: Arc::new(SecurityValidator::new()),
            audit_logger: Arc::new(AuditLogger::default()),
            registry: Arc::new(RwLock::new(PluginRegistry::new())),
            contexts: Arc::new(RwLock::new(std::collections::HashMap::new())),
            plugin_root: PathBuf::from("./plugins"),
            temp_root: PathBuf::from("./temp"),
            default_database_id: "default".to_string(),
            node_id: "default".to_string(),
            node_name: None,
            node_type: None,
            service_storage,
        })
    }
}
