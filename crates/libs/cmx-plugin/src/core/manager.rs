//! 插件管理器模块
//!
//! 作为核心协调器，协调各子模块完成生命周期操作。
//!
//! # 设计思想
//!
//! PluginManager 是插件系统的核心入口点，负责：
//! - 统一管理所有生命周期服务（安装、卸载、激活、升级、降级、回滚）
//! - 协调基础设施组件（数据库、缓存、存储、消息）
//! - 提供插件运行时环境管理
//! - 支持集群模式下的插件管理
//!
//! # 使用示例
//!
//! ```rust,no_run
//! use cmx_plugin::core::manager::PluginManager;
//! use cmx_plugin::config::settings::PluginManagerSettings;
//!
//! async fn example() {
//!     let settings = PluginManagerSettings::default();
//!     let manager = PluginManager::new(settings).await.unwrap();
//!
//!     // 安装插件
//!     let install_req = cmx_plugin::service::install::InstallRequest {
//!         source: cmx_plugin::domain::plugin::PluginSource::Local {
//!             path: std::path::PathBuf::from("./my-plugin.zip"),
//!         },
//!         db_id: None,
//!         force: false,
//!         auto_activate: false,
//!     };
//!     let result = manager.install(install_req).await.unwrap();
//! }
//! ```

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::audit::logger::AuditLogger;
use crate::cluster::deployment::DeploymentCoordinator;
use crate::cluster::node::NodeManager;
use crate::cluster::sync::SyncManager;
use crate::config::settings::PluginManagerSettings;
use crate::core::context::PluginContext;
use crate::core::lifecycle::{LifecycleState, LifecycleStateMachine};
use crate::core::registry::PluginRegistry;
use crate::domain::plugin::{PluginFilter, PluginInfo, PluginSource, PluginStatus};
use crate::domain::status::StatusTransition;
use crate::error::{PluginError, PluginResult};
use crate::infrastructure::cache::layered::LayeredCacheManager;
use crate::infrastructure::database::repository::PluginRepository;
use crate::infrastructure::database::schema::SchemaManager;
use crate::infrastructure::messaging::event::{Event, EventBus, EventType};
use crate::infrastructure::storage::TempDirCleanup;
use crate::infrastructure::storage::backup::BackupManager;
use crate::infrastructure::storage::file::FileStorage;
use crate::runtime::activation::ActivationManager;
use crate::runtime::feature::FeatureManager;
use crate::runtime::service_registry::ServiceRegistry;
use crate::security::permission::PermissionManager;
use crate::security::signature::SignatureValidator;
use crate::security::validator::SecurityValidator;
use cmx_buffer::{CacheManager, LockManager, PubSubOps};
use cmx_core::model::cell::TableDefine;
use cmx_core::model::meta::base::TableDefineDbExecutor;
use cmx_database::DatabaseManager;
use cmx_metadata::config::{
    TableDefinesConfigManager,
    load_table_defines_config_from_path,
};

pub use crate::service::activate::{
    ActivateRequest, ActivateResponse, DeactivateRequest, DeactivateResponse,
};
pub use crate::service::downgrade::{DowngradeRequest, DowngradeResponse};
// 重导出服务请求/响应类型，方便使用
pub use crate::service::install::{InstallRequest, InstallResponse};
pub use crate::service::rollback::{RollbackRequest, RollbackResponse};
pub use crate::service::uninstall::{UninstallRequest, UninstallResponse};
pub use crate::service::upgrade::{UpgradeRequest, UpgradeResponse};

/// 插件管理器构建器
///
/// 用于逐步配置和创建 PluginManager 实例。
pub struct PluginManagerBuilder {
    settings: PluginManagerSettings,
    db_manager: Option<Arc<DatabaseManager>>,
    cache_manager: Option<Arc<CacheManager>>,
    lock_manager: Option<Arc<LockManager>>,
    pubsub: Option<Arc<PubSubOps>>,
}

impl PluginManagerBuilder {
    /// 创建新的构建器
    pub fn new(settings: PluginManagerSettings) -> Self {
        Self {
            settings,
            db_manager: None,
            cache_manager: None,
            lock_manager: None,
            pubsub: None,
        }
    }

    /// 设置数据库管理器
    pub fn with_database(mut self, db_manager: Arc<DatabaseManager>) -> Self {
        self.db_manager = Some(db_manager);
        self
    }

    /// 设置 Redis 缓存管理器
    pub fn with_cache(mut self, cache_manager: Arc<CacheManager>) -> Self {
        self.cache_manager = Some(cache_manager);
        self
    }

    /// 设置分布式锁管理器
    pub fn with_lock_manager(mut self, lock_manager: Arc<LockManager>) -> Self {
        self.lock_manager = Some(lock_manager);
        self
    }

    /// 设置消息订阅发布
    pub fn with_pubsub(mut self, pubsub: Arc<PubSubOps>) -> Self {
        self.pubsub = Some(pubsub);
        self
    }

    /// 构建插件管理器
    pub async fn build(self) -> PluginResult<PluginManager> {
        PluginManager::from_builder(self).await
    }
}

/// 插件管理器
///
/// 插件系统的核心协调器，统一管理插件生命周期操作。
pub struct PluginManager {
    /// 配置设置
    settings: PluginManagerSettings,

    // 核心组件
    /// 插件注册表
    registry: Arc<RwLock<PluginRegistry>>,
    /// 插件上下文映射
    contexts: Arc<RwLock<HashMap<String, PluginContext>>>,

    // 基础设施组件
    /// 数据仓库
    repository: Arc<PluginRepository>,
    /// 缓存管理器
    cache: Arc<LayeredCacheManager>,
    /// 文件存储
    storage: Arc<FileStorage>,
    /// 备份管理器
    backup_manager: Arc<BackupManager>,
    /// 事件总线
    event_bus: Arc<EventBus>,

    // 安全组件
    /// 安全验证器
    security_validator: Arc<SecurityValidator>,
    /// 签名验证器
    signature_validator: Arc<SignatureValidator>,
    /// 权限管理器
    permission_manager: Arc<PermissionManager>,

    // 运行时组件
    /// 激活管理器
    activation_manager: Arc<ActivationManager>,
    /// 服务注册表
    service_registry: Arc<ServiceRegistry>,
    /// 功能管理器
    feature_manager: Arc<FeatureManager>,

    // 审计组件
    /// 审计日志
    audit_logger: Arc<AuditLogger>,

    // 集群组件（可选）
    /// 节点管理器
    node_manager: Option<Arc<NodeManager>>,
    /// 部署协调器
    deployment_coordinator: Option<Arc<DeploymentCoordinator>>,
    /// 状态同步管理器
    sync_manager: Option<Arc<SyncManager>>,

    // 分布式组件（可选）
    /// 分布式锁管理器
    lock_manager: Option<Arc<LockManager>>,

    /// 是否已初始化
    initialized: Arc<RwLock<bool>>,
}

impl PluginManager {
    /// 创建新的插件管理器
    ///
    /// 使用默认配置创建插件管理器实例。
    pub async fn new(settings: PluginManagerSettings) -> PluginResult<Self> {
        let builder = PluginManagerBuilder::new(settings);
        Self::from_builder(builder).await
    }

    /// 从构建器创建插件管理器
    async fn from_builder(builder: PluginManagerBuilder) -> PluginResult<Self> {
        let settings = builder.settings;

        // 创建数据库管理器（如果没有提供）
        let db_manager = builder
            .db_manager
            .unwrap_or_else(|| Arc::new(DatabaseManager::new(Default::default())));

        // 创建数据仓库
        let repository = Arc::new(PluginRepository::new(
            db_manager.clone(),
            settings.default_database_id.clone(),
        ));

        // 创建缓存管理器
        let cache = Arc::new(LayeredCacheManager::new(Default::default()));

        // 创建文件存储
        let storage = Arc::new(FileStorage::new(&settings.plugin_root));

        // 创建备份管理器
        let backup_manager = Arc::new(BackupManager::new(settings.backup_root.clone()));

        // 创建事件总线
        let event_bus = Arc::new(EventBus::new());

        // 创建安全组件
        let security_validator = Arc::new(SecurityValidator::new());
        let signature_validator = Arc::new(SignatureValidator::new());
        let permission_manager = Arc::new(PermissionManager::new());

        // 创建运行时组件
        let activation_manager = Arc::new(ActivationManager::new());
        let service_registry = Arc::new(ServiceRegistry::new());
        let feature_manager = Arc::new(FeatureManager::new(
            service_registry.clone(),
            event_bus.clone(),
        ));

        // 创建审计日志
        let audit_logger = Arc::new(AuditLogger::default());

        // 创建集群组件（如果配置了）
        let (node_manager, deployment_coordinator, sync_manager) =
            if let Some(ref cluster_settings) = settings.cluster {
                let node_mgr = Arc::new(NodeManager::new(cluster_settings.node_id.clone()));
                let deployment_coord = Arc::new(DeploymentCoordinator::new(node_mgr.clone()));
                let sync_mgr = Arc::new(SyncManager::new(cluster_settings.node_id.clone()));
                (Some(node_mgr), Some(deployment_coord), Some(sync_mgr))
            } else {
                (None, None, None)
            };

        let manager = Self {
            settings,
            registry: Arc::new(RwLock::new(PluginRegistry::new())),
            contexts: Arc::new(RwLock::new(HashMap::new())),
            repository,
            cache,
            storage,
            backup_manager,
            event_bus,
            security_validator,
            signature_validator,
            permission_manager,
            activation_manager,
            service_registry,
            feature_manager,
            audit_logger,
            node_manager,
            deployment_coordinator,
            sync_manager,
            lock_manager: builder.lock_manager,
            initialized: Arc::new(RwLock::new(false)),
        };

        // 初始化
        manager.initialize().await?;

        Ok(manager)
    }

    /// 初始化插件管理器
    ///
    /// 执行系统表初始化、缓存预热等操作。
    pub async fn initialize(&self) -> PluginResult<()> {
        let mut initialized = self.initialized.write().await;
        if *initialized {
            return Ok(());
        }

        // 初始化系统表 fixme 暂不需要
        // self.repository.init_system_tables().await?;

        // 加载已安装插件到内存
        self.load_installed_plugins().await?;

        *initialized = true;

        // 发布初始化完成事件
        self.event_bus
            .publish(Event::new(
                EventType::SystemStarted,
                "plugin-manager".to_string(),
                serde_json::json!({
                    "timestamp": Utc::now().to_rfc3339(),
                }),
            ))
            .await;

        Ok(())
    }

    /// 加载已安装插件到内存
    async fn load_installed_plugins(&self) -> PluginResult<()> {
        let records = self
            .repository
            .list_plugins(&PluginFilter::default())
            .await?;

        let mut registry = self.registry.write().await;
        let mut contexts = self.contexts.write().await;

        for record in records {
            let context = PluginContext::from_db_record(&record);
            contexts.insert(record.plugin_id.clone(), context);

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
            registry.register(info);
        }

        Ok(())
    }

    // ==================== 生命周期操作 ====================

    /// 安装插件
    ///
    /// 执行完整的插件安装流程：
    /// 1. 获取插件包
    /// 2. 验证插件安全性
    /// 3. 解析插件定义
    /// 4. 检查已安装状态
    /// 5. 检查依赖
    /// 6. 创建安装目录
    /// 7. 复制文件
    /// 8. 创建数据库表
    /// 9. 注册插件
    /// 10. 保存数据库记录
    /// 11. 更新缓存
    /// 12. 记录审计日志
    pub async fn install(&self, request: InstallRequest) -> PluginResult<InstallResponse> {
        let start_time = std::time::Instant::now();
        let plugin_id = self.extract_plugin_id_from_source(&request.source).await?;

        // 使用分布式锁（如果可用）
        self.with_lock(&format!("plugin:install:{}", plugin_id), || async {
            self.do_install(request, start_time).await
        })
        .await
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
    async fn do_install(
        &self,
        request: InstallRequest,
        start_time: std::time::Instant,
    ) -> PluginResult<InstallResponse> {
        // 步骤1：获取插件包
        let package_path = self
            .fetch_package(&request.source, request.version_constraint.as_deref())
            .await?;

        // 步骤2：准备临时目录（如果是 zip 则解压，如果是文件夹则直接使用）
        let temp_dir = self
            .settings
            .temp_root
            .join(format!("plugin_install_{}", uuid::Uuid::new_v4()));
        let (extract_path, needs_cleanup) = self
            .prepare_package_for_validation(&package_path, &temp_dir)
            .await?;

        // 使用 RAII 确保临时目录被清理
        let _cleanup = TempDirCleanup::new(needs_cleanup.then_some(temp_dir.clone()));

        // 步骤3：验证插件安全性（在解压后的目录进行）
        let validation_result = self
            .security_validator
            .validate_plugin_package(&extract_path)
            .await;
        if !validation_result.passed {
            let errors = validation_result.errors.join(", ");
            return Err(PluginError::Security(format!("安全验证失败: {}", errors)));
        }

        // 步骤4：解析插件定义（在解压后的目录进行）
        let plugin_def = self.parse_plugin_definition(&extract_path).await?;
        let plugin_id = plugin_def.id.clone();
        let version = plugin_def
            .version
            .clone()
            .unwrap_or_else(|| "1.0.0".to_string());

        // 步骤5：检查已安装状态
        if !request.force {
            if self.is_plugin_installed(&plugin_id).await? {
                return Err(PluginError::plugin_already_exists(&plugin_id));
            }
        }

        // 步骤6：检查依赖
        let dep_result = self.check_plugin_dependencies(&plugin_def).await?;
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

        // 步骤7：创建安装目录
        let install_path = self.settings.plugin_root.join(&plugin_id);
        if install_path.exists() && request.force {
            self.storage.remove_dir(&install_path)?;
        }
        self.storage.create_dir(&install_path)?;

        // 步骤8：复制文件（从解压目录到安装目录）
        self.copy_plugin_files(&extract_path, &install_path).await?;

        // 步骤9：创建数据库表（如果需要）
        let db_id = request
            .db_id
            .clone()
            .unwrap_or_else(|| self.settings.default_database_id.clone());
        if !plugin_def.table_config_files.is_empty() {
            self.create_plugin_tables(&plugin_def, &db_id, &install_path)
                .await?;
        }

        // 步骤10：注册插件
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

        // 步骤11：保存数据库记录
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
        };

        self.repository.insert_plugin(&db_record).await?;

        // 更新注册表和上下文
        {
            let mut registry = self.registry.write().await;
            registry.register(plugin_info.clone());
        }

        {
            let mut contexts = self.contexts.write().await;
            let context = PluginContext::from_db_record(&db_record);
            contexts.insert(plugin_id.clone(), context);
        }

        // 步骤12：更新缓存
        self.cache
            .set(
                &format!("plugin:{}", plugin_id),
                crate::infrastructure::cache::layered::CacheValue::Json(
                    serde_json::to_value(&plugin_info).unwrap(),
                ),
                None,
            )
            .await;

        // 步骤13：记录审计日志
        let audit_record = crate::audit::record::AuditRecord::success(
            plugin_id.clone(),
            crate::audit::record::OperationType::Install,
        )
        .with_details(serde_json::json!({
            "version": version,
            "install_path": install_path.to_string_lossy().to_string(),
            "duration_ms": start_time.elapsed().as_millis(),
        }));
        self.audit_logger.log(audit_record).await;

        // 发布事件
        self.event_bus
            .publish(Event::new(
                EventType::PluginInstalled,
                plugin_id.clone(),
                serde_json::json!({
                    "version": version,
                    "install_path": install_path.to_string_lossy().to_string(),
                }),
            ))
            .await;

        // 如果需要自动激活
        if request.auto_activate {
            let activate_req = ActivateRequest {
                plugin_id: plugin_id.clone(),
                force: false,
            };
            self.activate(activate_req).await?;
        }

        Ok(InstallResponse {
            plugin_id,
            install_path,
            success: true,
            message: "插件安装成功".to_string(),
        })
    }

    /// 准备插件包用于验证
    ///
    /// 如果是 zip 包，解压到临时目录；如果是文件夹，直接返回路径。
    ///
    /// # 返回值
    /// - `(extract_path, needs_cleanup)`: 解压后的路径和是否需要清理
    async fn prepare_package_for_validation(
        &self,
        package_path: &std::path::Path,
        temp_dir: &std::path::Path,
    ) -> PluginResult<(std::path::PathBuf, bool)> {
        // 判断是 zip 还是文件夹
        let is_zip = package_path
            .extension()
            .map(|ext| ext == "zip")
            .unwrap_or(false);

        if is_zip {
            // 创建临时目录
            std::fs::create_dir_all(temp_dir)
                .map_err(|e| PluginError::Install(format!("创建临时目录失败: {}", e)))?;

            // 解压到临时目录
            self.extract_zip(package_path, temp_dir).await?;

            // 查找解压后的实际目录（可能 zip 内有根目录）
            let extract_path = self.find_plugin_root_in_dir(temp_dir)?;

            tracing::info!("插件包已解压到临时目录: {}", extract_path.display());

            Ok((extract_path, true))
        } else if package_path.is_dir() {
            // 已经是文件夹，直接使用
            Ok((package_path.to_path_buf(), false))
        } else {
            Err(PluginError::Install(format!(
                "不支持的插件包格式: {}",
                package_path.display()
            )))
        }
    }

    /// 在解压目录中查找插件根目录
    ///
    /// ZIP 包可能包含一个根目录，需要找到包含 manifest.json 的目录。
    fn find_plugin_root_in_dir(&self, dir: &std::path::Path) -> PluginResult<std::path::PathBuf> {
        // 首先检查当前目录是否有 manifest.json
        if dir.join("manifest.json").exists() {
            return Ok(dir.to_path_buf());
        }

        // 遍历子目录查找 manifest.json
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if path.join("manifest.json").exists() {
                        return Ok(path);
                    }
                    // 递归查找（支持多层嵌套）
                    if let Ok(found) = self.find_plugin_root_in_dir(&path) {
                        return Ok(found);
                    }
                }
            }
        }

        // 如果找不到 manifest.json，返回原目录（后续验证会报错）
        Ok(dir.to_path_buf())
    }

    /// 卸载插件
    ///
    /// 执行完整的插件卸载流程：
    /// 1. 检查插件存在
    /// 2. 检查依赖
    /// 3. 停用插件
    /// 4. 创建备份（可选）
    /// 5. 删除文件
    /// 6. 清理数据库记录
    /// 7. 清除缓存
    /// 8. 记录审计日志
    pub async fn uninstall(&self, request: UninstallRequest) -> PluginResult<UninstallResponse> {
        let start_time = std::time::Instant::now();

        // 使用分布式锁（如果可用）
        self.with_lock(
            &format!("plugin:uninstall:{}", request.plugin_id),
            || async { self.do_uninstall(request, start_time).await },
        )
        .await
    }

    /// 执行卸载操作
    async fn do_uninstall(
        &self,
        request: UninstallRequest,
        start_time: std::time::Instant,
    ) -> PluginResult<UninstallResponse> {
        // 步骤1：检查插件存在
        let plugin = self
            .repository
            .find_plugin(&request.plugin_id)
            .await?
            .ok_or_else(|| PluginError::plugin_not_found(&request.plugin_id))?;

        // 步骤2：检查依赖（非强制模式）
        if !request.force {
            let dependents = self.check_dependents(&request.plugin_id).await?;
            if !dependents.is_empty() {
                return Err(PluginError::Dependency(format!(
                    "插件 {} 被以下插件依赖: {}",
                    request.plugin_id,
                    dependents.join(", ")
                )));
            }
        }

        // 步骤3：停用插件（如果已激活）
        if plugin.status == "activated" {
            let deactivate_req = DeactivateRequest {
                plugin_id: request.plugin_id.clone(),
                force: request.force,
            };
            self.deactivate(deactivate_req).await?;
        }

        // 步骤4：创建备份（如果保留数据）
        if request.keep_data {
            let install_path = PathBuf::from(&plugin.install_path);
            if install_path.exists() {
                self.backup_manager
                    .create_backup(&request.plugin_id, &plugin.version, &install_path)
                    .await
                    .map_err(|e| PluginError::Uninstall(format!("创建备份失败: {}", e)))?;
            }
        }

        // 步骤5：删除文件
        let install_path = PathBuf::from(&plugin.install_path);
        if install_path.exists() && !request.keep_config {
            self.storage
                .remove_dir(&install_path)
                .map_err(|e| PluginError::Uninstall(format!("删除插件文件失败: {}", e)))?;
        }

        // 步骤6：清理数据库记录
        self.repository.delete_plugin(&request.plugin_id).await?;

        // 更新注册表和上下文
        {
            let mut registry = self.registry.write().await;
            registry.unregister(&request.plugin_id);
        }

        {
            let mut contexts = self.contexts.write().await;
            contexts.remove(&request.plugin_id);
        }

        // 步骤7：清除缓存
        self.cache
            .delete(&format!("plugin:{}", request.plugin_id))
            .await;

        // 步骤8：记录审计日志
        let audit_record = crate::audit::record::AuditRecord::success(
            request.plugin_id.clone(),
            crate::audit::record::OperationType::Uninstall,
        )
        .with_details(serde_json::json!({
            "version": plugin.version,
            "keep_config": request.keep_config,
            "keep_data": request.keep_data,
            "duration_ms": start_time.elapsed().as_millis(),
        }));
        self.audit_logger.log(audit_record).await;

        // 发布事件
        self.event_bus
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

    /// 激活插件
    ///
    /// 执行完整的插件激活流程：
    /// 1. 检查插件存在
    /// 2. 检查当前状态
    /// 3. 检查依赖是否已激活
    /// 4. 加载 WASM 模块
    /// 5. 注册服务
    /// 6. 更新状态
    /// 7. 记录审计日志
    pub async fn activate(&self, request: ActivateRequest) -> PluginResult<ActivateResponse> {
        let start_time = std::time::Instant::now();

        // 使用分布式锁（如果可用）
        self.with_lock(
            &format!("plugin:activate:{}", request.plugin_id),
            || async { self.do_activate(request, start_time).await },
        )
        .await
    }

    /// 执行激活操作
    async fn do_activate(
        &self,
        request: ActivateRequest,
        start_time: std::time::Instant,
    ) -> PluginResult<ActivateResponse> {
        // 步骤1：检查插件存在
        let plugin = self
            .repository
            .find_plugin(&request.plugin_id)
            .await?
            .ok_or_else(|| PluginError::plugin_not_found(&request.plugin_id))?;

        // 步骤2：检查当前状态
        if plugin.status == "activated" {
            return Ok(ActivateResponse {
                plugin_id: request.plugin_id,
                success: true,
                message: "插件已经处于激活状态".to_string(),
            });
        }

        if plugin.status != "installed" && plugin.status != "deactivated" && !request.force {
            return Err(PluginError::invalid_state(
                &request.plugin_id,
                &plugin.status,
                "activate",
            ));
        }

        // 步骤3：检查依赖是否已激活（非强制模式）
        if !request.force {
            let inactive_deps = self
                .check_dependencies_activated(&request.plugin_id)
                .await?;
            if !inactive_deps.is_empty() {
                return Err(PluginError::Dependency(format!(
                    "以下依赖尚未激活: {}",
                    inactive_deps.join(", ")
                )));
            }
        }

        // 步骤4：加载 WASM 模块
        self.activation_manager
            .activate(&request.plugin_id, &plugin.version)
            .await
            .map_err(|e| PluginError::Activate(format!("加载 WASM 模块失败: {}", e)))?;

        // 步骤5：注册服务（如果插件提供服务）
        self.register_plugin_services(&request.plugin_id, &plugin.install_path)
            .await?;

        // 步骤6：更新状态
        let mut fields = crate::infrastructure::database::repository::PluginUpdateFields {
            status: Some("activated".to_string()),
            ..Default::default()
        };
        fields.activated_at = Some(Utc::now());
        self.repository
            .update_plugin(&request.plugin_id, &fields)
            .await?;

        // 更新上下文
        {
            let mut contexts = self.contexts.write().await;
            if let Some(context) = contexts.get_mut(&request.plugin_id) {
                context.status = PluginStatus::Activated;
                context.activated_at = Some(Utc::now());
            }
        }

        // 更新缓存
        self.cache
            .delete(&format!("plugin:{}", request.plugin_id))
            .await;

        // 步骤7：记录审计日志
        let audit_record = crate::audit::record::AuditRecord::success(
            request.plugin_id.clone(),
            crate::audit::record::OperationType::Activate,
        )
        .with_details(serde_json::json!({
            "version": plugin.version,
            "duration_ms": start_time.elapsed().as_millis(),
        }));
        self.audit_logger.log(audit_record).await;

        // 发布事件
        self.event_bus
            .publish(Event::new(
                EventType::PluginActivated,
                request.plugin_id.clone(),
                serde_json::json!({
                    "version": plugin.version,
                }),
            ))
            .await;

        Ok(ActivateResponse {
            plugin_id: request.plugin_id,
            success: true,
            message: "插件激活成功".to_string(),
        })
    }

    /// 停用插件
    ///
    /// 执行完整的插件停用流程：
    /// 1. 检查插件存在
    /// 2. 检查当前状态
    /// 3. 检查是否有其他插件依赖此插件
    /// 4. 注销服务
    /// 5. 卸载 WASM 模块
    /// 6. 更新状态
    /// 7. 记录审计日志
    pub async fn deactivate(&self, request: DeactivateRequest) -> PluginResult<DeactivateResponse> {
        let start_time = std::time::Instant::now();

        // 使用分布式锁（如果可用）
        self.with_lock(
            &format!("plugin:deactivate:{}", request.plugin_id),
            || async { self.do_deactivate(request, start_time).await },
        )
        .await
    }

    /// 执行停用操作
    async fn do_deactivate(
        &self,
        request: DeactivateRequest,
        start_time: std::time::Instant,
    ) -> PluginResult<DeactivateResponse> {
        // 步骤1：检查插件存在
        let plugin = self
            .repository
            .find_plugin(&request.plugin_id)
            .await?
            .ok_or_else(|| PluginError::plugin_not_found(&request.plugin_id))?;

        // 步骤2：检查当前状态
        if plugin.status != "activated" {
            return Ok(DeactivateResponse {
                plugin_id: request.plugin_id,
                success: true,
                message: "插件已经处于停用状态".to_string(),
            });
        }

        // 步骤3：检查是否有其他插件依赖此插件（非强制模式）
        if !request.force {
            let dependents = self.check_active_dependents(&request.plugin_id).await?;
            if !dependents.is_empty() {
                return Err(PluginError::Dependency(format!(
                    "以下已激活的插件依赖此插件: {}",
                    dependents.join(", ")
                )));
            }
        }

        // 步骤4：注销服务
        self.service_registry
            .unregister_plugin_services(&request.plugin_id)
            .await;

        // 步骤5：卸载 WASM 模块
        self.activation_manager
            .deactivate(&request.plugin_id)
            .await
            .map_err(|e| PluginError::Deactivate(format!("卸载 WASM 模块失败: {}", e)))?;

        // 步骤6：更新状态
        self.repository
            .update_plugin_status(&request.plugin_id, "deactivated")
            .await?;

        // 更新上下文
        {
            let mut contexts = self.contexts.write().await;
            if let Some(context) = contexts.get_mut(&request.plugin_id) {
                context.status = PluginStatus::Deactivated;
            }
        }

        // 更新缓存
        self.cache
            .delete(&format!("plugin:{}", request.plugin_id))
            .await;

        // 步骤7：记录审计日志
        let audit_record = crate::audit::record::AuditRecord::success(
            request.plugin_id.clone(),
            crate::audit::record::OperationType::Deactivate,
        )
        .with_details(serde_json::json!({
            "version": plugin.version,
            "duration_ms": start_time.elapsed().as_millis(),
        }));
        self.audit_logger.log(audit_record).await;

        // 发布事件
        self.event_bus
            .publish(Event::new(
                EventType::PluginDeactivated,
                request.plugin_id.clone(),
                serde_json::json!({
                    "version": plugin.version,
                }),
            ))
            .await;

        Ok(DeactivateResponse {
            plugin_id: request.plugin_id,
            success: true,
            message: "插件停用成功".to_string(),
        })
    }

    /// 升级插件
    ///
    /// 执行完整的插件升级流程：
    /// 1. 检查插件存在
    /// 2. 验证版本
    /// 3. 创建备份
    /// 4. 下载新版本
    /// 5. 停用旧版本
    /// 6. 安装新版本
    /// 7. 迁移数据
    /// 8. 激活新版本
    /// 9. 记录审计日志
    pub async fn upgrade(&self, request: UpgradeRequest) -> PluginResult<UpgradeResponse> {
        let start_time = std::time::Instant::now();

        // 使用分布式锁（如果可用）
        self.with_lock(&format!("plugin:upgrade:{}", request.plugin_id), || async {
            self.do_upgrade(request, start_time).await
        })
        .await
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
    /// 8. 安装新版本
    /// 9. 更新数据库记录
    /// 10. 重新激活（如需要）
    /// 11. 清理临时目录
    async fn do_upgrade(
        &self,
        request: UpgradeRequest,
        start_time: std::time::Instant,
    ) -> PluginResult<UpgradeResponse> {
        // 步骤1：检查插件存在
        let plugin = self
            .repository
            .find_plugin(&request.plugin_id)
            .await?
            .ok_or_else(|| PluginError::plugin_not_found(&request.plugin_id))?;

        let old_version = plugin.version.clone();

        // 步骤2：获取新版本插件包
        let package_path = self
            .fetch_package(&request.source, request.version_constraint.as_deref())
            .await?;

        // 步骤3：准备临时目录（如果是 zip 则解压，如果是文件夹则直接使用）
        let temp_dir = self
            .settings
            .temp_root
            .join(format!("plugin_upgrade_{}", uuid::Uuid::new_v4()));
        let (extract_path, needs_cleanup) = self
            .prepare_package_for_validation(&package_path, &temp_dir)
            .await?;

        // 使用 RAII 确保临时目录被清理
        let _cleanup = TempDirCleanup::new(needs_cleanup.then_some(temp_dir.clone()));

        // 步骤4：安全验证
        let validation_result = self
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
            .backup_manager
            .create_backup(&request.plugin_id, &old_version, &install_path)
            .await
            .map_err(|e| PluginError::Upgrade(format!("创建备份失败: {}", e)))?;

        // 步骤8：停用插件（如果已激活）
        let was_activated = plugin.status == "activated";
        if was_activated {
            let deactivate_req = DeactivateRequest {
                plugin_id: request.plugin_id.clone(),
                force: true,
            };
            self.deactivate(deactivate_req).await?;
        }

        // 步骤9：删除旧文件
        if install_path.exists() {
            self.storage
                .remove_dir(&install_path)
                .map_err(|e| PluginError::Upgrade(format!("删除旧文件失败: {}", e)))?;
        }

        // 步骤10：安装新版本（从解压目录复制）
        self.storage.create_dir(&install_path)?;
        self.copy_plugin_files(&extract_path, &install_path).await?;

        // 步骤11：更新数据库记录
        let fields = crate::infrastructure::database::repository::PluginUpdateFields {
            version: Some(new_version.clone()),
            ..Default::default()
        };
        self.repository
            .update_plugin(&request.plugin_id, &fields)
            .await?;

        // 步骤12：如果之前是激活状态，重新激活
        if was_activated || request.auto_activate {
            let activate_req = ActivateRequest {
                plugin_id: request.plugin_id.clone(),
                force: true,
            };
            self.activate(activate_req).await?;
        }

        // 步骤13：记录审计日志
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
        self.audit_logger.log(audit_record).await;

        // 发布事件
        self.event_bus
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

    /// 降级插件
    ///
    /// 执行完整的插件降级流程：
    /// 1. 检查插件存在
    /// 2. 验证版本
    /// 3. 创建备份
    /// 4. 停用当前版本
    /// 5. 恢复旧版本备份
    /// 6. 激活旧版本
    /// 7. 记录审计日志
    pub async fn downgrade(&self, request: DowngradeRequest) -> PluginResult<DowngradeResponse> {
        let start_time = std::time::Instant::now();

        // 使用分布式锁（如果可用）
        self.with_lock(
            &format!("plugin:downgrade:{}", request.plugin_id),
            || async { self.do_downgrade(request, start_time).await },
        )
        .await
    }

    /// 执行降级操作
    async fn do_downgrade(
        &self,
        request: DowngradeRequest,
        start_time: std::time::Instant,
    ) -> PluginResult<DowngradeResponse> {
        // 步骤1：检查插件存在
        let plugin = self
            .repository
            .find_plugin(&request.plugin_id)
            .await?
            .ok_or_else(|| PluginError::plugin_not_found(&request.plugin_id))?;

        let current_version = plugin.version.clone();

        // 步骤2：查找目标版本的备份
        let backups = self
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
        self.backup_manager
            .create_backup(&request.plugin_id, &current_version, &install_path)
            .await
            .map_err(|e| PluginError::Downgrade(format!("创建备份失败: {}", e)))?;

        // 步骤4：停用插件（如果已激活）
        let was_activated = plugin.status == "activated";
        if was_activated {
            let deactivate_req = DeactivateRequest {
                plugin_id: request.plugin_id.clone(),
                force: true,
            };
            self.deactivate(deactivate_req).await?;
        }

        // 步骤5：恢复旧版本
        if install_path.exists() {
            self.storage
                .remove_dir(&install_path)
                .map_err(|e| PluginError::Downgrade(format!("删除当前文件失败: {}", e)))?;
        }

        self.backup_manager
            .restore_backup(&target_backup.path, &install_path)
            .await
            .map_err(|e| PluginError::Downgrade(format!("恢复备份失败: {}", e)))?;

        // 步骤6：更新数据库记录
        let fields = crate::infrastructure::database::repository::PluginUpdateFields {
            version: Some(request.target_version.clone()),
            ..Default::default()
        };
        self.repository
            .update_plugin(&request.plugin_id, &fields)
            .await?;

        // 步骤7：如果之前是激活状态，重新激活
        if was_activated {
            let activate_req = ActivateRequest {
                plugin_id: request.plugin_id.clone(),
                force: true,
            };
            self.activate(activate_req).await?;
        }

        // 步骤8：记录审计日志
        let audit_record = crate::audit::record::AuditRecord::success(
            request.plugin_id.clone(),
            crate::audit::record::OperationType::Downgrade,
        )
        .with_details(serde_json::json!({
            "from_version": current_version,
            "to_version": request.target_version,
            "duration_ms": start_time.elapsed().as_millis(),
        }));
        self.audit_logger.log(audit_record).await;

        // 发布事件
        self.event_bus
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

    /// 回滚插件
    ///
    /// 执行插件回滚操作，恢复到上一个版本。
    pub async fn rollback(&self, request: RollbackRequest) -> PluginResult<RollbackResponse> {
        let start_time = std::time::Instant::now();

        // 步骤1：检查插件存在
        let plugin = self
            .repository
            .find_plugin(&request.plugin_id)
            .await?
            .ok_or_else(|| PluginError::plugin_not_found(&request.plugin_id))?;

        let current_version = plugin.version.clone();

        // 步骤2：获取最近的备份
        let backups = self
            .backup_manager
            .list_backups(&request.plugin_id)
            .await
            .map_err(|e| PluginError::Rollback(format!("获取备份列表失败: {}", e)))?;

        // 排除当前版本，获取最近的备份
        let target_backup = backups
            .into_iter()
            .filter(|b| b.version != current_version)
            .next()
            .ok_or_else(|| PluginError::Rollback("没有可回滚的备份".to_string()))?;

        let target_version = target_backup.version.clone();
        let plugin_id = request.plugin_id.clone();

        // 使用降级功能执行回滚
        let downgrade_req = DowngradeRequest {
            plugin_id: request.plugin_id,
            target_version: target_backup.version,
            force: true,
            auto_activate: request.auto_activate,
            keep_backup: true,
        };

        self.downgrade(downgrade_req).await?;

        // 记录审计日志
        let audit_record = crate::audit::record::AuditRecord::success(
            plugin_id.clone(),
            crate::audit::record::OperationType::Rollback,
        )
        .with_details(serde_json::json!({
            "from_version": current_version,
            "to_version": target_version,
            "duration_ms": start_time.elapsed().as_millis(),
        }));
        self.audit_logger.log(audit_record).await;

        Ok(RollbackResponse {
            plugin_id,
            from_version: current_version,
            to_version: target_version,
            success: true,
            message: "插件回滚成功".to_string(),
        })
    }

    // ==================== 查询操作 ====================

    /// 获取插件信息
    pub async fn get_plugin(&self, plugin_id: &str) -> PluginResult<Option<PluginInfo>> {
        // 先查内存
        {
            let registry = self.registry.read().await;
            if let Some(info) = registry.get(plugin_id) {
                return Ok(Some(info.clone()));
            }
        }

        // 再查数据库
        if let Some(record) = self.repository.find_plugin(plugin_id).await? {
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

    /// 获取插件上下文
    pub async fn get_context(&self, plugin_id: &str) -> Option<PluginContext> {
        let contexts = self.contexts.read().await;
        contexts.get(plugin_id).cloned()
    }

    /// 列出所有插件
    pub async fn list_plugins(&self, filter: &PluginFilter) -> PluginResult<Vec<PluginInfo>> {
        let registry = self.registry.read().await;
        Ok(registry.list_all())
    }

    /// 检查插件是否已安装
    pub async fn is_plugin_installed(&self, plugin_id: &str) -> PluginResult<bool> {
        let registry = self.registry.read().await;
        Ok(registry.contains(plugin_id))
    }

    /// 检查插件是否已激活
    pub async fn is_plugin_activated(&self, plugin_id: &str) -> PluginResult<bool> {
        Ok(self.activation_manager.is_active(plugin_id).await)
    }

    /// 获取插件生命周期状态
    pub async fn get_lifecycle_state(&self, plugin_id: &str) -> PluginResult<LifecycleState> {
        if let Some(context) = self.get_context(plugin_id).await {
            match context.status {
                PluginStatus::Installed => Ok(LifecycleState::Installed),
                PluginStatus::Activated => Ok(LifecycleState::Activated),
                PluginStatus::Deactivated => Ok(LifecycleState::Deactivated),
                PluginStatus::Error => Ok(LifecycleState::Error),
            }
        } else {
            Ok(LifecycleState::NotInstalled)
        }
    }

    /// 获取有效的状态转换
    pub fn get_valid_transitions(&self, state: LifecycleState) -> Vec<LifecycleState> {
        LifecycleStateMachine::valid_transitions(state)
    }

    /// 检查状态转换是否有效
    pub fn can_transition(&self, from: LifecycleState, to: LifecycleState) -> bool {
        LifecycleStateMachine::can_transition(from, to)
    }

    // ==================== 辅助方法 ====================

    /// 从插件来源提取插件ID
    async fn extract_plugin_id_from_source(&self, source: &PluginSource) -> PluginResult<String> {
        match source {
            PluginSource::Local { path } => {
                // 从路径提取插件ID
                let file_name = path
                    .file_stem()
                    .ok_or_else(|| PluginError::Install("无法从路径提取插件ID".to_string()))?
                    .to_string_lossy()
                    .to_string();
                Ok(file_name)
            }
            PluginSource::Remote { url, .. } => {
                // 从URL提取插件ID
                let url_parsed = url::Url::parse(url)
                    .map_err(|e| PluginError::Install(format!("解析URL失败: {}", e)))?;
                let file_name = url_parsed
                    .path_segments()
                    .and_then(|segments| segments.last())
                    .ok_or_else(|| PluginError::Install("无法从URL提取插件ID".to_string()))?
                    .trim_end_matches(".zip");
                Ok(file_name.to_string())
            }
            PluginSource::Registry { package_name, .. } => Ok(package_name.clone()),
        }
    }

    /// 获取插件包
    async fn fetch_package(
        &self,
        source: &PluginSource,
        version_constraint: Option<&str>,
    ) -> PluginResult<PathBuf> {
        match source {
            PluginSource::Local { path } => {
                let fetcher = crate::fetcher::local::LocalFetcher::new(&self.settings.plugin_root);
                fetcher
                    .fetch(&crate::fetcher::source::PluginSource::local(path.clone()))
                    .await
                    .map_err(|e| PluginError::Install(format!("获取本地插件包失败: {}", e)))
            }
            PluginSource::Remote { url, checksum } => {
                let fetcher =
                    crate::fetcher::remote::RemoteFetcher::new(self.settings.temp_root.clone());
                fetcher
                    .fetch(&crate::fetcher::source::PluginSource::remote(
                        url.clone(),
                        checksum.clone(),
                    ))
                    .await
                    .map_err(|e| PluginError::Install(format!("获取远程插件包失败: {}", e)))
            }
            PluginSource::Registry {
                registry_url,
                package_name,
            } => {
                let fetcher = crate::fetcher::registry::RegistryFetcher::new(
                    crate::fetcher::registry::RegistryInfo::new(
                        registry_url.clone().unwrap_or_default(),
                    ),
                    self.settings.temp_root.clone(),
                );
                fetcher
                    .fetch_by_name(package_name, version_constraint.map(|s| s.to_string()))
                    .await
                    .map_err(|e| PluginError::Install(format!("从注册表获取插件包失败: {}", e)))
            }
        }
    }

    /// 解析插件定义
    ///
    /// 从 manifest.json 中解析插件定义。
    ///
    /// manifest.json 结构：
    /// ```json
    /// {
    ///   "manifest_version": "1.0",
    ///   "plugin": {
    ///     "id": "example_plugin",
    ///     "name": "示例插件",
    ///     "version": "1.0.0",
    ///     "main_file": "bin/plugin.wasm",
    ///     ...
    ///   }
    /// }
    /// ```
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

        // 先解析为 JSON Value 以获取 plugin 对象
        let manifest: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| PluginError::Metadata(format!("解析 manifest.json 失败: {}", e)))?;

        // 获取 plugin 对象
        let plugin_value = manifest
            .get("plugin")
            .ok_or_else(|| PluginError::Metadata("manifest.json 缺少 plugin 对象".to_string()))?;

        // 将 plugin 对象解析为 PluginDefinition
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
            self.storage
                .copy_dir(source, target)
                .map_err(|e| PluginError::Install(format!("复制插件文件失败: {}", e)))?;
        } else if source.extension().map(|e| e == "zip").unwrap_or(false) {
            // 解压 ZIP 文件
            self.extract_zip(source, target).await?;
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
            .map_err(|e| PluginError::Install(format!("解压插件包失败: {}", e)))?;

        Ok(())
    }

    /// 创建插件数据库表
    ///
    /// 使用 cmx-metadata 解析表定义并创建数据库表。
    async fn create_plugin_tables(
        &self,
        plugin_def: &cmx_core::model::meta::plugin::PluginDefinition,
        db_id: &str,
        install_path: &std::path::Path,
    ) -> PluginResult<()> {
        // 如果没有表配置文件，直接返回
        if plugin_def.table_config_files.is_empty() {
            return Ok(());
        }

        let mut table_config_manager = TableDefinesConfigManager::new();
        let executor = cmx_metadata::PgTableDefineExecutor::new(db_id, None);
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

        //开始创建表
        // let table_defs = table_config_manager.load_all_tables(install_path)
        //     .map_err(|e| PluginError::Metadata(format!("加载表定义失败: {}", e)))?;

        // 遍历所有表配置文件
        // for table_config_file in &plugin_def.table_config_files {
        //     let config_path = install_path.join(table_config_file);
        //
        //     if !config_path.exists() {
        //         tracing::warn!(
        //             "表配置文件不存在: {}",
        //             config_path.display()
        //         );
        //         continue;
        //     }
        //
        //     // 读取配置文件内容
        //     let config_content = std::fs::read_to_string(&config_path)
        //         .map_err(|e| PluginError::Metadata(format!(
        //             "读取表配置文件失败: {} - {}",
        //             config_path.display(),
        //             e
        //         )))?;
        //
        //
        //
        //     // 解析表定义
        //     let table_def: cmx_core::model::cell::TableDefine =
        //         serde_json::from_str(&config_content)
        //             .or_else(|e| {
        //                 // `e` 就是 serde_json::from_str 返回的错误
        //                 eprintln!("TableDefine JSON 解析失败: {}", e);
        //
        //             Err(     PluginError::Metadata(format!(
        //                 "解析表配置文件失败: {} - {}",
        //                 config_path.display(),
        //                 e
        //             )))
        //                 // // 尝试作为 TOML 解析
        //                 // toml::from_str(&config_content)
        //                 //     .map_err(|e| PluginError::Metadata(format!(
        //                 //         "解析表配置文件失败: {} - {}",
        //                 //         config_path.display(),
        //                 //         e
        //                 //     )))
        //             })?;
        //
        //     // 创建表执行器
        //     let executor = cmx_metadata::PgTableDefineExecutor::new(db_id, None);
        //
        //     // 执行建表
        //     use cmx_core::model::meta::base::TableDefineDbExecutor;
        //     executor.create_table(&table_def)
        //         .map_err(|e| PluginError::Metadata(format!(
        //             "创建表失败: {} - {}",
        //             table_def.table_name,
        //             e
        //         )))?;
        //
        //     tracing::info!(
        //         "成功创建插件表: {} ({})",
        //         table_def.table_name,
        //         plugin_def.id
        //     );
        // }

        Ok(())
    }

    /// 检查插件依赖
    ///
    /// 验证插件的所有依赖是否已安装且版本满足约束。
    async fn check_plugin_dependencies(
        &self,
        plugin_def: &cmx_core::model::meta::plugin::PluginDefinition,
    ) -> PluginResult<crate::domain::dependency::DependencyCheckResult> {
        use crate::domain::dependency::{
            DependencyCheckResult, DependencyConflict, MissingDependency,
        };

        let mut result = DependencyCheckResult::new();

        for dep in &plugin_def.dependencies {
            if dep.optional {
                continue;
            }

            let installed = self.is_plugin_installed(&dep.plugin_id).await?;

            if !installed {
                let version_constraint = dep
                    .version_constraint
                    .as_ref()
                    .and_then(|v| crate::domain::version::VersionConstraint::parse(v).ok());

                result.add_missing(MissingDependency {
                    plugin_id: dep.plugin_id.clone(),
                    version_constraint,
                    required_by: plugin_def.id.clone(),
                });
                continue;
            }

            if let Some(ref constraint_str) = dep.version_constraint {
                if let Ok(constraint) =
                    crate::domain::version::VersionConstraint::parse(constraint_str)
                {
                    if let Some(plugin_info) = self.get_plugin(&dep.plugin_id).await? {
                        if let Ok(installed_version) =
                            crate::domain::version::SemanticVersion::parse(&plugin_info.version)
                        {
                            if !constraint.satisfies(&installed_version) {
                                result.add_conflict(DependencyConflict {
                                    plugin_id: dep.plugin_id.clone(),
                                    constraints: vec![(plugin_def.id.clone(), constraint)],
                                });
                            }
                        }
                    }
                }
            }
        }

        Ok(result)
    }

    /// 注册插件服务
    ///
    /// 从插件定义中获取服务列表并注册到服务注册表。
    async fn register_plugin_services(
        &self,
        plugin_id: &str,
        install_path: &str,
    ) -> PluginResult<()> {
        let install_path = std::path::PathBuf::from(install_path);
        let manifest_path = install_path.join("manifest.json");

        if !manifest_path.exists() {
            tracing::debug!("插件 {} 没有 manifest.json 文件，跳过服务注册", plugin_id);
            return Ok(());
        }

        let content = std::fs::read_to_string(&manifest_path)
            .map_err(|e| PluginError::Activate(format!("读取插件定义失败: {}", e)))?;

        let plugin_def: cmx_core::model::meta::plugin::PluginDefinition =
            serde_json::from_str(&content)
                .map_err(|e| PluginError::Activate(format!("解析插件定义失败: {}", e)))?;

        if plugin_def.services.is_empty() {
            tracing::debug!("插件 {} 没有定义服务，跳过服务注册", plugin_id);
            return Ok(());
        }

        for service in &plugin_def.services {
            let service_def = crate::runtime::service_registry::ServiceDefinition {
                id: service.service_id.clone(),
                name: service.name.clone(),
                provider_plugin_id: plugin_id.to_string(),
                service_type: "wasm".to_string(),
                config: Some(serde_json::json!({
                    "entry_point": service.entry_point,
                    "version": service.version,
                    "description": service.description,
                })),
            };

            if let Err(e) = self.service_registry.register(service_def).await {
                tracing::warn!("注册服务 {} 失败: {}", service.service_id, e);
            } else {
                tracing::info!("成功注册服务: {} (插件: {})", service.service_id, plugin_id);
            }
        }

        Ok(())
    }

    /// 检查依赖此插件的其他插件
    ///
    /// 查询所有插件，检查它们的依赖列表中是否包含当前插件。
    async fn check_dependents(&self, plugin_id: &str) -> PluginResult<Vec<String>> {
        let all_plugins = self
            .repository
            .list_plugins(&PluginFilter::default())
            .await?;
        let mut dependents = Vec::new();

        for plugin in all_plugins {
            // 从元数据中获取依赖信息
            if let Some(ref metadata) = plugin.metadata {
                if let Some(deps) = metadata.get("dependencies").and_then(|d| d.as_array()) {
                    for dep in deps {
                        if let Some(dep_id) = dep.get("plugin_id").and_then(|id| id.as_str()) {
                            if dep_id == plugin_id {
                                dependents.push(plugin.plugin_id.clone());
                                break;
                            }
                        }
                    }
                }
            }
        }

        Ok(dependents)
    }

    /// 检查依赖是否已激活
    ///
    /// 获取插件的依赖列表，检查每个依赖是否已激活。
    async fn check_dependencies_activated(&self, plugin_id: &str) -> PluginResult<Vec<String>> {
        let plugin = self
            .repository
            .find_plugin(plugin_id)
            .await?
            .ok_or_else(|| PluginError::plugin_not_found(plugin_id))?;

        let mut inactive_deps = Vec::new();

        // 从元数据中获取依赖信息
        if let Some(ref metadata) = plugin.metadata {
            if let Some(deps) = metadata.get("dependencies").and_then(|d| d.as_array()) {
                for dep in deps {
                    if let Some(dep_id) = dep.get("plugin_id").and_then(|id| id.as_str()) {
                        // 检查依赖是否已安装且激活
                        if let Some(dep_plugin) = self.repository.find_plugin(dep_id).await? {
                            if dep_plugin.status != "activated" {
                                inactive_deps.push(dep_id.to_string());
                            }
                        } else {
                            // 依赖未安装
                            inactive_deps.push(dep_id.to_string());
                        }
                    }
                }
            }
        }

        Ok(inactive_deps)
    }

    /// 检查已激活的依赖此插件的其他插件
    ///
    /// 查询所有已激活的插件，检查它们的依赖列表中是否包含当前插件。
    async fn check_active_dependents(&self, plugin_id: &str) -> PluginResult<Vec<String>> {
        let all_plugins = self
            .repository
            .list_plugins(&PluginFilter::default())
            .await?;
        let mut dependents = Vec::new();

        for plugin in all_plugins {
            // 只检查已激活的插件
            if plugin.status != "activated" {
                continue;
            }

            // 从元数据中获取依赖信息
            if let Some(ref metadata) = plugin.metadata {
                if let Some(deps) = metadata.get("dependencies").and_then(|d| d.as_array()) {
                    for dep in deps {
                        if let Some(dep_id) = dep.get("plugin_id").and_then(|id| id.as_str()) {
                            if dep_id == plugin_id {
                                dependents.push(plugin.plugin_id.clone());
                                break;
                            }
                        }
                    }
                }
            }
        }

        Ok(dependents)
    }

    /// 使用分布式锁执行操作
    async fn with_lock<F, Fut, T>(&self, lock_key: &str, f: F) -> PluginResult<T>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = PluginResult<T>>,
    {
        if let Some(ref lock_manager) = self.lock_manager {
            let _guard = lock_manager
                .lock(lock_key)
                .await
                .map_err(|e| PluginError::Plugin(format!("获取分布式锁失败: {}", e)))?;
            f().await
        } else {
            f().await
        }
    }

    // ==================== 组件访问器 ====================

    /// 获取数据仓库
    pub fn repository(&self) -> &Arc<PluginRepository> {
        &self.repository
    }

    /// 获取缓存管理器
    pub fn cache(&self) -> &Arc<LayeredCacheManager> {
        &self.cache
    }

    /// 获取事件总线
    pub fn event_bus(&self) -> &Arc<EventBus> {
        &self.event_bus
    }

    /// 获取激活管理器
    pub fn activation_manager(&self) -> &Arc<ActivationManager> {
        &self.activation_manager
    }

    /// 获取服务注册表
    pub fn service_registry(&self) -> &Arc<ServiceRegistry> {
        &self.service_registry
    }

    /// 获取功能管理器
    pub fn feature_manager(&self) -> &Arc<FeatureManager> {
        &self.feature_manager
    }

    /// 获取节点管理器
    pub fn node_manager(&self) -> Option<&Arc<NodeManager>> {
        self.node_manager.as_ref()
    }

    /// 获取部署协调器
    pub fn deployment_coordinator(&self) -> Option<&Arc<DeploymentCoordinator>> {
        self.deployment_coordinator.as_ref()
    }

    /// 获取审计日志
    pub fn audit_logger(&self) -> &Arc<AuditLogger> {
        &self.audit_logger
    }

    /// 获取配置设置
    pub fn settings(&self) -> &PluginManagerSettings {
        &self.settings
    }

    /// 关闭插件管理器
    ///
    /// 执行清理操作，包括停用所有插件、释放资源等。
    pub async fn shutdown(&self) -> PluginResult<()> {
        // 停用所有已激活的插件
        let active_plugins = self.activation_manager.get_active_plugins().await;
        for plugin_id in active_plugins {
            let deactivate_req = DeactivateRequest {
                plugin_id,
                force: true,
            };
            let _ = self.deactivate(deactivate_req).await;
        }

        // 发布关闭事件
        self.event_bus
            .publish(Event::new(
                EventType::SystemStopped,
                "plugin-manager".to_string(),
                serde_json::json!({
                    "timestamp": Utc::now().to_rfc3339(),
                }),
            ))
            .await;

        Ok(())
    }
}

impl Default for PluginManager {
    fn default() -> Self {
        // 注意：这里不能使用 async，所以使用 block_on
        // 实际使用时应该使用 PluginManager::new()
        let settings = PluginManagerSettings::default();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                Self::new(settings)
                    .await
                    .expect("创建默认 PluginManager 失败")
            })
        })
    }
}
