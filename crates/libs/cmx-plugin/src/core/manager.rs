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
///
/// # 设计思想
///
/// PluginManager 作为协调器，负责：
/// - 持有和协调各个 Service（InstallService、UpgradeService 等）
/// - 提供统一的 API 入口
/// - 管理共享的基础设施组件
///
/// 具体的业务逻辑由各个 Service 实现。
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

    // 运行时组件
    /// 激活管理器
    activation_manager: Arc<ActivationManager>,
    /// 服务注册表
    service_registry: Arc<ServiceRegistry>,

    // 审计组件
    /// 审计日志
    audit_logger: Arc<AuditLogger>,

    // 集群组件（可选）
    /// 节点管理器
    node_manager: Option<Arc<NodeManager>>,
    /// 部署协调器
    deployment_coordinator: Option<Arc<DeploymentCoordinator>>,

    // 服务组件
    /// 安装服务
    install_service: crate::service::install::InstallService,
    /// 升级服务
    upgrade_service: crate::service::upgrade::UpgradeService,
    /// 激活服务
    activate_service: crate::service::activate::ActivateService,
    /// 卸载服务
    uninstall_service: crate::service::uninstall::UninstallService,
    /// 降级服务
    downgrade_service: crate::service::downgrade::DowngradeService,
    /// 回滚服务
    rollback_service: crate::service::rollback::RollbackService,

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

        // 创建安全验证器
        let security_validator = Arc::new(SecurityValidator::new());

        // 创建激活管理器
        let activation_manager = Arc::new(ActivationManager::new());

        // 创建服务注册表
        let service_registry = Arc::new(ServiceRegistry::new());

        // 创建审计日志
        let audit_logger = Arc::new(AuditLogger::default());

        // 创建插件注册表和上下文
        let registry = Arc::new(RwLock::new(PluginRegistry::new()));
        let contexts = Arc::new(RwLock::new(HashMap::new()));

        // 创建集群组件（如果配置了）
        let (node_manager, deployment_coordinator) =
            if let Some(ref cluster_settings) = settings.cluster {
                let node_mgr = Arc::new(NodeManager::new(cluster_settings.node_id.clone()));
                let deployment_coord = Arc::new(DeploymentCoordinator::new(node_mgr.clone()));
                (Some(node_mgr), Some(deployment_coord))
            } else {
                (None, None)
            };

        // 创建各个服务
        let install_service = crate::service::install::InstallService::new(
            crate::service::install::InstallServiceDeps {
                repository: repository.clone(),
                cache: cache.clone(),
                storage: storage.clone(),
                backup_manager: backup_manager.clone(),
                security_validator: security_validator.clone(),
                event_bus: event_bus.clone(),
                audit_logger: audit_logger.clone(),
                registry: registry.clone(),
                contexts: contexts.clone(),
                plugin_root: settings.plugin_root.clone(),
                temp_root: settings.temp_root.clone(),
                default_database_id: settings.default_database_id.clone(),
            }
        );

        let upgrade_service = crate::service::upgrade::UpgradeService::new(
            crate::service::upgrade::UpgradeServiceDeps {
                repository: repository.clone(),
                cache: cache.clone(),
                storage: storage.clone(),
                backup_manager: backup_manager.clone(),
                security_validator: security_validator.clone(),
                event_bus: event_bus.clone(),
                audit_logger: audit_logger.clone(),
                registry: registry.clone(),
                contexts: contexts.clone(),
                plugin_root: settings.plugin_root.clone(),
                temp_root: settings.temp_root.clone(),
            }
        );

        let activate_service = crate::service::activate::ActivateService::new(
            crate::service::activate::ActivateServiceDeps {
                repository: repository.clone(),
                cache: cache.clone(),
                storage: storage.clone(),
                event_bus: event_bus.clone(),
                audit_logger: audit_logger.clone(),
                activation_manager: activation_manager.clone(),
                service_registry: service_registry.clone(),
                contexts: contexts.clone(),
            }
        );

        let uninstall_service = crate::service::uninstall::UninstallService::new(
            crate::service::uninstall::UninstallServiceDeps {
                repository: repository.clone(),
                cache: cache.clone(),
                storage: storage.clone(),
                backup_manager: backup_manager.clone(),
                event_bus: event_bus.clone(),
                audit_logger: audit_logger.clone(),
                registry: registry.clone(),
                contexts: contexts.clone(),
            }
        );

        let downgrade_service = crate::service::downgrade::DowngradeService::new(
            crate::service::downgrade::DowngradeServiceDeps {
                repository: repository.clone(),
                storage: storage.clone(),
                backup_manager: backup_manager.clone(),
                event_bus: event_bus.clone(),
                audit_logger: audit_logger.clone(),
                contexts: contexts.clone(),
            }
        );

        let rollback_service = crate::service::rollback::RollbackService::new(
            crate::service::rollback::RollbackServiceDeps {
                repository: repository.clone(),
                cache: cache.clone(),
                storage: storage.clone(),
                backup_manager: backup_manager.clone(),
                event_bus: event_bus.clone(),
                audit_logger: audit_logger.clone(),
                contexts: contexts.clone(),
            }
        );

        let manager = Self {
            settings,
            registry,
            contexts,
            repository,
            cache,
            storage,
            backup_manager,
            event_bus,
            security_validator,
            activation_manager,
            service_registry,
            audit_logger,
            node_manager,
            deployment_coordinator,
            install_service,
            upgrade_service,
            activate_service,
            uninstall_service,
            downgrade_service,
            rollback_service,
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

        //删除临时文件夹
        self.storage.remove_dir(&self.settings.temp_root)
            .unwrap_or_else(|e| log::error!("删除临时目录{:?}失败: {}",&self.settings.temp_root, e));


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
        self.install_service.install(request).await
            .map_err(|e| PluginError::Install(format!("安装失败: {}", e)))
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
        self.uninstall_service.uninstall(request).await
            .map_err(|e| PluginError::Uninstall(format!("卸载失败: {}", e)))
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
        self.activate_service.activate(request).await
            .map_err(|e| PluginError::Activate(format!("激活失败: {}", e)))
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
        self.activate_service.deactivate(request).await
            .map_err(|e| PluginError::Deactivate(format!("停用失败: {}", e)))
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
        self.upgrade_service.upgrade(request).await
            .map_err(|e| PluginError::Upgrade(format!("升级失败: {}", e)))
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
        self.downgrade_service.downgrade(request).await
            .map_err(|e| PluginError::Downgrade(format!("降级失败: {}", e)))
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

