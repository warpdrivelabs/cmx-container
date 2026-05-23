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

use crate::audit::logger::{AuditLogger, AuditLoggerConfig};
use crate::cluster::deployment::DeploymentCoordinator;
use crate::cluster::node::NodeManager;
use crate::common::{
    DependencyUtils, DependencyUtilsDeps, ServiceUtils, ServiceUtilsDeps,
};
use crate::config::settings::PluginManagerSettings;
use crate::core::context::PluginContext;
use crate::core::lifecycle::{LifecycleState, LifecycleStateMachine};
use crate::core::registry::PluginRegistry;
use crate::domain::plugin::{PluginFilter, PluginInfo, PluginSource, PluginStatus};
use crate::error::PluginResult;
use crate::infrastructure::cache::layered::LayeredCacheManager;
use crate::infrastructure::database::repository::PluginRepository;
use crate::infrastructure::database::version_history::VersionHistoryRepository;
use crate::infrastructure::storage::backup::BackupManager;
use crate::infrastructure::storage::file::FileStorage;
use crate::runtime::activation::ActivationManager;
use crate::runtime::service_registry::ServiceRegistry;
use crate::security::validator::SecurityValidator;
use cmx_buffer::{CacheManager, GlobalCacheManager, GlobalLockManager, LockManager, PubSubOps};
use cmx_database::{DatabaseManager, get_default_db_manager};
use cmx_service::{GlobalServiceQuery, GlobalServiceStorage};
use tokio::sync::RwLock;
use tracing::error;

pub use crate::service::activate::{
    ActivateRequest, ActivateResponse, DeactivateRequest, DeactivateResponse,
};
pub use crate::service::deploy::{DeployAction, DeployRequest, DeployResponse};
pub use crate::service::downgrade::{DowngradeRequest, DowngradeResponse};
pub use crate::service::install::{InstallRequest, InstallResponse};
pub use crate::service::rollback::{RollbackRequest, RollbackResponse};
pub use crate::service::uninstall::{UninstallRequest, UninstallResponse};
pub use crate::service::upgrade::{UpgradeRequest, UpgradeResponse};

/// 插件管理器构建器
///
/// 用于逐步配置和创建 PluginManager 实例。
///
/// # 示例
///
/// ```rust,no_run
/// use cmx_plugin::core::manager::PluginManagerBuilder;
/// use cmx_plugin::config::settings::PluginManagerSettings;
///
/// let builder = PluginManagerBuilder::new(PluginManagerSettings::default())
///     .with_database(cmx_database::get_default_db_manager().clone());
/// ```
pub struct PluginManagerBuilder {
    /// 配置设置
    settings: PluginManagerSettings,
    /// 数据库管理器
    db_manager: Option<Arc<DatabaseManager>>,
    /// Redis 缓存管理器
    cache_manager: Option<Arc<CacheManager>>,
    /// 分布式锁管理器
    lock_manager: Option<Arc<LockManager>>,
    /// 消息订阅发布
    pubsub: Option<Arc<PubSubOps>>,
}

impl PluginManagerBuilder {
    /// 创建新的构建器
    ///
    /// # 参数
    ///
    /// * `settings` - 插件管理器配置设置
    ///
    /// # 返回值
    ///
    /// 返回初始化后的构建器实例，已预设默认的数据库、缓存和锁管理器。
    pub fn new(settings: PluginManagerSettings) -> Self {
        Self {
            settings,
            db_manager: Some(get_default_db_manager().clone()),
            cache_manager: Some(GlobalCacheManager::get().clone()),
            lock_manager: Some(GlobalLockManager::get().clone()),
            pubsub: Some(Arc::new(GlobalCacheManager::get().pubsub())),
        }
    }

    /// 设置数据库管理器
    ///
    /// # 参数
    ///
    /// * `db_manager` - 数据库管理器实例
    pub fn with_database(mut self, db_manager: Arc<DatabaseManager>) -> Self {
        self.db_manager = Some(db_manager);
        self
    }

    /// 设置 Redis 缓存管理器
    ///
    /// # 参数
    ///
    /// * `cache_manager` - Redis 缓存管理器实例
    pub fn with_cache(mut self, cache_manager: Arc<CacheManager>) -> Self {
        self.cache_manager = Some(cache_manager);
        self
    }

    /// 设置分布式锁管理器
    ///
    /// # 参数
    ///
    /// * `lock_manager` - 分布式锁管理器实例
    pub fn with_lock_manager(mut self, lock_manager: Arc<LockManager>) -> Self {
        self.lock_manager = Some(lock_manager);
        self
    }

    /// 设置消息订阅发布
    ///
    /// # 参数
    ///
    /// * `pubsub` - 消息订阅发布实例
    pub fn with_pubsub(mut self, pubsub: Arc<PubSubOps>) -> Self {
        self.pubsub = Some(pubsub);
        self
    }

    /// 构建插件管理器
    ///
    /// # 返回值
    ///
    /// 返回构建完成的 PluginManager 实例。
    ///
    /// # 错误
    ///
    /// - 初始化失败时返回错误
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
#[allow(dead_code)]
pub struct PluginManager {
    /// 配置设置
    settings: PluginManagerSettings,
    /// 应用ID
    app_id: String,

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
    /// 插件变更通知器（可选）
    plugin_notifier: Option<Arc<crate::cluster::notification::PluginNotifier>>,

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

    /// 部署服务（智能安装/升级）
    deploy_service: crate::service::deploy::DeployService,

    /// 管控服务（集中式插件管理，不触发本地运行时加载）
    control_service: crate::service::control::ControlService,

    // 初始化组件
    /// 插件初始化器（用于启动时同步）
    plugin_initializer: crate::service::initializer::PluginInitializer,

    /// 运行时加载器（处理 RuntimeLoad/RuntimeUnload 通知）
    runtime_loader: Arc<crate::service::runtime_loader::RuntimeLoader>,

    // 通用工具
    /// 依赖检查工具
    dependency_utils: DependencyUtils,
    /// 服务注册工具
    service_utils: ServiceUtils,

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

        // 创建插件变更通知器（如果 Redis Pub/Sub 可用）
        let pubsub_for_notifier = builder.pubsub.clone();
        let plugin_notifier: Option<Arc<crate::cluster::notification::PluginNotifier>> =
            pubsub_for_notifier.map(|ps| Arc::new(crate::cluster::notification::PluginNotifier::new(ps)));

        let db_manager = builder
            .db_manager
            .unwrap_or_else(|| Arc::new(DatabaseManager::new(Default::default())));

        let repository = Arc::new(PluginRepository::new(
            db_manager.clone(),
            settings.default_database_id.clone(),
        ));

        let version_history_repository = Arc::new(VersionHistoryRepository::new(
            db_manager.clone(),
            settings.default_database_id.clone(),
        ));

        let cache = Arc::new(LayeredCacheManager::new(Default::default()));

        let storage = Arc::new(FileStorage::new(&settings.plugin_root));

        let backup_manager = Arc::new(BackupManager::new(settings.backup_root.clone()));

        let security_validator = Arc::new(SecurityValidator::new());

        let activation_manager = Arc::new(ActivationManager::new());

        let service_registry = Arc::new(ServiceRegistry::new());

        let audit_logger_config = AuditLoggerConfig::new(
            db_manager.clone(),
            settings.default_database_id.clone(),
            settings.node_id.clone().unwrap_or_else(|| "default".to_string()),
        );
        let audit_logger = Arc::new(AuditLogger::new(audit_logger_config));

        let registry = Arc::new(RwLock::new(PluginRegistry::new()));
        let contexts = Arc::new(RwLock::new(HashMap::new()));

        let (node_manager, deployment_coordinator) =
            if let Some(ref cluster_settings) = settings.cluster {
                let node_mgr = Arc::new(NodeManager::new(cluster_settings.node_id.clone()));
                let deployment_coord = Arc::new(DeploymentCoordinator::new(node_mgr.clone()));
                (Some(node_mgr), Some(deployment_coord))
            } else {
                (None, None)
            };

        let dependency_utils = DependencyUtils::new(DependencyUtilsDeps {
            repository: repository.clone(),
            registry: registry.clone(),
        });

        let service_utils = ServiceUtils::new(ServiceUtilsDeps {
            service_registry: service_registry.clone(),
        });

        let install_service = crate::service::install::InstallService::new(
            crate::service::install::InstallServiceDeps {
                repository: repository.clone(),
                version_history_repository: version_history_repository.clone(),
                cache: cache.clone(),
                storage: storage.clone(),
                backup_manager: backup_manager.clone(),
                security_validator: security_validator.clone(),
                audit_logger: audit_logger.clone(),
                registry: registry.clone(),
                contexts: contexts.clone(),
                plugin_root: settings.plugin_root.clone(),
                temp_root: settings.temp_root.clone(),
                default_database_id: settings.default_database_id.clone(),
                node_name: settings.node_name.clone(),
                node_type: settings.node_type.clone(),
                service_storage: GlobalServiceStorage::get().clone(),
                plugin_notifier: plugin_notifier.clone(),
                lock_manager: builder.lock_manager.clone(),
            },
        );

        let upgrade_service = crate::service::upgrade::UpgradeService::new(
            crate::service::upgrade::UpgradeServiceDeps {
                repository: repository.clone(),
                version_history_repository: version_history_repository.clone(),
                cache: cache.clone(),
                storage: storage.clone(),
                backup_manager: backup_manager.clone(),
                security_validator: security_validator.clone(),
                audit_logger: audit_logger.clone(),
                registry: registry.clone(),
                contexts: contexts.clone(),
                plugin_root: settings.plugin_root.clone(),
                temp_root: settings.temp_root.clone(),
                default_database_id: settings.default_database_id.clone(),
                node_name: settings.node_name.clone(),
                node_type: settings.node_type.clone(),
                service_storage: GlobalServiceStorage::get().clone(),
                plugin_notifier: plugin_notifier.clone(),
                lock_manager: builder.lock_manager.clone(),
            },
        );

        let activate_service = crate::service::activate::ActivateService::new(
            crate::service::activate::ActivateServiceDeps {
                repository: repository.clone(),
                cache: cache.clone(),
                storage: storage.clone(),
                audit_logger: audit_logger.clone(),
                activation_manager: activation_manager.clone(),
                service_registry: service_registry.clone(),
                contexts: contexts.clone(),
            },
        );

        let uninstall_service = crate::service::uninstall::UninstallService::new(
            crate::service::uninstall::UninstallServiceDeps {
                repository: repository.clone(),
                version_history_repository: version_history_repository.clone(),
                cache: cache.clone(),
                audit_logger: audit_logger.clone(),
                registry: registry.clone(),
                contexts: contexts.clone(),
                service_storage: GlobalServiceStorage::get().clone(),
                plugin_notifier: plugin_notifier.clone(),
            },
        );

        let downgrade_service = crate::service::downgrade::DowngradeService::new(
            crate::service::downgrade::DowngradeServiceDeps {
                repository: repository.clone(),
                version_history_repository: version_history_repository.clone(),
                cache: cache.clone(),
                audit_logger: audit_logger.clone(),
                registry: registry.clone(),
                plugin_root: settings.plugin_root.clone(),
                default_database_id: settings.default_database_id.clone(),
                service_query: GlobalServiceQuery::get().clone(),
                service_storage: GlobalServiceStorage::get().clone(),
                plugin_notifier: plugin_notifier.clone(),
            },
        );

        let rollback_service = crate::service::rollback::RollbackService::new(
            crate::service::rollback::RollbackServiceDeps {
                repository: repository.clone(),
                cache: cache.clone(),
                storage: storage.clone(),
                backup_manager: backup_manager.clone(),
                audit_logger: audit_logger.clone(),
                contexts: contexts.clone(),
            },
        );

        let deploy_service =
            crate::service::deploy::DeployService::new(crate::service::deploy::DeployServiceDeps {
                repository: repository.clone(),
                cache: cache.clone(),
                storage: storage.clone(),
                security_validator: security_validator.clone(),
                install_service: install_service.clone(),
                upgrade_service: upgrade_service.clone(),
                uninstall_service: uninstall_service.clone(),
                plugin_notifier: plugin_notifier.clone(),
                plugin_root: settings.plugin_root.clone(),
                temp_root: settings.temp_root.clone(),
            });

        let control_service = crate::service::control::ControlService::new(
            crate::service::control::ControlServiceDeps {
                install_service: install_service.clone(),
                upgrade_service: upgrade_service.clone(),
                downgrade_service: downgrade_service.clone(),
                uninstall_service: uninstall_service.clone(),
                notifier: plugin_notifier.clone(),
                app_id: settings.app_id.clone(),
                repository: repository.clone(),
                plugin_root: settings.plugin_root.clone(),
                temp_root: settings.temp_root.clone(),
                storage: storage.clone(),
            }
        );


        let runtime_loader = Arc::new(
            crate::service::runtime_loader::RuntimeLoader::new(
                repository.clone(),
                registry.clone(),
                contexts.clone(),
                settings.plugin_root.clone(),
                settings.app_id.clone(),
                settings.temp_root.clone(),
            ),
        );

        // 创建插件初始化器（在 manager 之前创建，使用 clone 避免 move）
        let plugin_initializer = crate::service::initializer::PluginInitializer::new(
            crate::service::initializer::PluginInitializerDeps {
                repository: repository.clone(),
                version_history_repository: version_history_repository.clone(),
                registry: registry.clone(),
                contexts: contexts.clone(),
                install_service: install_service.clone(),
                upgrade_service: upgrade_service.clone(),
                downgrade_service: downgrade_service.clone(),
                uninstall_service: uninstall_service.clone(),
                plugin_root: settings.plugin_root.clone(),
                app_id: settings.app_id.clone(),
            }
        );



        let app_id = settings.app_id.clone();

        let manager = Self {
            settings,
            app_id,
            registry,
            contexts,
            repository,
            cache,
            storage,
            backup_manager,
            security_validator,
            activation_manager,
            service_registry,
            audit_logger,
            node_manager,
            deployment_coordinator,
            plugin_notifier,
            install_service,
            upgrade_service,
            activate_service,
            uninstall_service,
            downgrade_service,
            rollback_service,
            deploy_service,
            control_service,
            plugin_initializer,
            runtime_loader,
            dependency_utils,
            service_utils,
            initialized: Arc::new(RwLock::new(false)),
        };

        manager.initialize().await?;

        Ok(manager)
    }

    /// 初始化插件管理器
    ///
    /// 执行系统表初始化、自动安装、缓存预热等操作。
    pub async fn initialize(&self) -> PluginResult<()> {
        let mut initialized = self.initialized.write().await;
        if *initialized {
            return Ok(());
        }

        self.storage
            .remove_dir(&self.settings.temp_root)
            .unwrap_or_else(|e| {
                tracing::error!("删除临时目录{:?}失败: {}", &self.settings.temp_root, e)
            });

        // // 执行自动安装：在 sync_plugins 之前，确保配置中声明的插件已安装
        // if self.settings.auto_install.enabled {
        //     let auto_install_service = crate::service::auto_install::AutoInstallService::new(
        //         self.repository.clone(),
        //         self.install_service.clone(),
        //         self.upgrade_service.clone(),
        //         self.app_id.clone(),
        //     );
        //     let result = auto_install_service.run(&self.settings.auto_install).await;
        //     match result {
        //         Ok(r) => {
        //             tracing::info!(
        //                 "插件自动安装完成: 安装={}, 升级={}, 跳过={}, 失败={}",
        //                 r.installed.len(),
        //                 r.upgraded.len(),
        //                 r.skipped.len(),
        //                 r.failed.len()
        //             );
        //             for (plugin_id, err) in &r.failed {
        //                 tracing::error!("插件 {} 自动安装失败: {}", plugin_id, err);
        //             }
        //             if r.has_critical_failure {
        //                 return Err(crate::error::PluginError::Install(
        //                     "关键插件自动安装失败，终止启动".to_string(),
        //                 ));
        //             }
        //         }
        //         Err(e) => {
        //             return Err(crate::error::PluginError::Install(format!(
        //                 "插件自动安装执行失败: {:?}",
        //                 e
        //             )));
        //         }
        //     }
        // }
        //
        // // 启动时同步插件：对比 cmx_plugin 表与本地文件系统
        // // 执行安装/升级/降级/卸载操作，然后加载 contexts 到内存
        // let sync_result = self.plugin_initializer.sync_plugins().await?;
        // tracing::info!(
        //     "插件同步完成: 安装={}, 升级={}, 降级={}, 卸载={}, 跳过={}, 失败={}",
        //     sync_result.installed.len(),
        //     sync_result.upgraded.len(),
        //     sync_result.downgraded.len(),
        //     sync_result.uninstalled.len(),
        //     sync_result.skipped.len(),
        //     sync_result.failed.len()
        // );
        // for (plugin_id, err) in &sync_result.failed {
        //     tracing::error!("插件 {} 同步失败: {}", plugin_id, err);
        // }

        // 0522加载 数据库插件到context和registry
        self.plugin_initializer.load_contexts().await?;

        // 启动 Redis Pub/Sub 订阅，监听跨实例插件变更通知
        // 使用 GlobalSubscriber 统一管理订阅，内置自动重连和自动重新订阅
        if let Some(ref _pubsub) = self.plugin_notifier {
            let handler = crate::service::plugin_sync::PluginChangeHandler::new(
                self.repository.clone(),
                self.deploy_service.clone(),
                self.settings.plugin_root.clone(),
                self.registry.clone(),
                self.contexts.clone(),
                self.settings.app_id.clone(),
                self.runtime_loader.clone(),
            );
            let handler = Arc::new(handler);

            let full_channel = crate::cluster::notification::PLUGIN_CHANGE_CHANNEL.to_string();

            // 初始化全局订阅者
            if !cmx_buffer::GlobalSubscriberManager::is_initialized() {
                cmx_buffer::GlobalSubscriberManager::initialize().await?;
            }

            let subscriber = cmx_buffer::GlobalSubscriberManager::get();
            subscriber.register_channel_fn(&full_channel, move |channel, payload| {
                tracing::debug!(channel = %channel, "收到 Redis Pub/Sub 消息");
                let handler = handler.clone();
                let payload = payload.to_string();
                tokio::spawn(async move {
                    match serde_json::from_str::<crate::cluster::notification::PluginChangeNotification>(&payload) {
                        Ok(notification) => {
                            tracing::info!(
                                plugin_id = %notification.plugin_id,
                                action = ?notification.action,
                                timestamp = %notification.timestamp,
                                "收到插件变更通知，开始处理"
                            );
                            handler.handle(&notification).await;
                        }
                        Err(e) => {
                            tracing::debug!("解析插件变更通知失败: {}", e);
                        }
                    }
                });
            }).await?;

            tracing::info!("已启动插件变更通知订阅（GlobalSubscriber + 自动重连）");
        }

        // 启动定时对账任务（对比 DB 与本地 Registry，自动补偿差异）
        if self.settings.reconciliation_interval_secs > 0 {
            let recon = crate::service::reconciliation::ReconciliationTask::new(
                self.repository.clone(),
                self.registry.clone(),
                self.runtime_loader.clone(),
                self.settings.app_id.clone(),
                self.settings.reconciliation_interval_secs,
                self.settings.plugin_root.clone(),
            );
            let recon = Arc::new(recon);
            recon.clone().start();
            tracing::info!(
                "已启动插件定时对账任务（间隔 {}s, app_id={}）",
                self.settings.reconciliation_interval_secs,
                self.settings.app_id
            );

            //5.22 yqs启动初始化不在执行插件的ddl等逻辑，只执行插件下载解压
            // self.runtime_loader().
            // recon.clone().reconcile().await?;
        }

        *initialized = true;

        Ok(())
    }

    // ==================== 生命周期操作 ==================== start

    /// 安装插件
    pub async fn install(&self, request: InstallRequest) -> PluginResult<InstallResponse> {
        self.install_service.install(request).await.map_err(|e| {
            error!("安装失败: {}", e);
            e
        })
    }

    /// 卸载插件
    pub async fn uninstall(&self, request: UninstallRequest) -> PluginResult<UninstallResponse> {
        self.uninstall_service
            .uninstall(request)
            .await
            .map_err(|e| {
                error!("卸载失败: {}", e);
                e
            })
    }

    // /// 激活插件
    // pub async fn activate(&self, request: ActivateRequest) -> PluginResult<ActivateResponse> {
    //     self.activate_service
    //         .activate(request)
    //         .await
    //         .map_err(|e| PluginError::Activate(format!("激活失败: {}", e)))
    // }
    //
    // /// 停用插件
    // pub async fn deactivate(&self, request: DeactivateRequest) -> PluginResult<DeactivateResponse> {
    //     self.activate_service
    //         .deactivate(request)
    //         .await
    //         .map_err(|e| PluginError::Deactivate(format!("停用失败: {}", e)))
    // }

    /// 升级插件
    pub async fn upgrade(&self, request: UpgradeRequest) -> PluginResult<UpgradeResponse> {
        self.upgrade_service.upgrade(request).await.map_err(|e| {
            error!("升级失败: {}", e);
            e
        })
    }

    /// 降级插件
    pub async fn downgrade(&self, request: DowngradeRequest) -> PluginResult<DowngradeResponse> {
        self.downgrade_service
            .downgrade(request)
            .await
            .map_err(|e| {
                error!("降级失败: {}", e);
                e
            })
    }

    /// 部署插件（自动判断安装/升级/覆盖安装）
    ///
    /// 根据当前插件安装状态和版本比较结果，自动选择执行安装、升级或覆盖安装操作。
    pub async fn deploy(&self, request: DeployRequest) -> PluginResult<DeployResponse> {
        self.deploy_service.deploy(request).await.map_err(|e| {
            error!("部署失败: {}", e);
            e
        })
    }

    // /// 回滚插件
    // pub async fn rollback(&self, request: RollbackRequest) -> PluginResult<RollbackResponse> {
    //     let start_time = std::time::Instant::now();
    //
    //     let plugin = self
    //         .repository
    //         .find_plugin(&request.plugin_id)
    //         .await?
    //         .ok_or_else(|| PluginError::plugin_not_found(&request.plugin_id))?;
    //
    //     let current_version = plugin.version.clone();
    //
    //     let backups = self
    //         .backup_manager
    //         .list_backups(&request.plugin_id)
    //         .await
    //         .map_err(|e| PluginError::Rollback(format!("获取备份列表失败: {}", e)))?;
    //
    //     let target_backup = backups
    //         .into_iter()
    //         .filter(|b| b.version != current_version)
    //         .next()
    //         .ok_or_else(|| PluginError::Rollback("没有可回滚的备份".to_string()))?;
    //
    //     let target_version = target_backup.version.clone();
    //     let plugin_id = request.plugin_id.clone();
    //
    //     let downgrade_req = DowngradeRequest {
    //         plugin_id: request.plugin_id,
    //         target_version: target_backup.version,
    //         source: None,
    //         operator: "system".to_string(),
    //     };
    //
    //     self.downgrade(downgrade_req).await?;
    //
    //     let audit_record = crate::audit::record::AuditRecord::success(
    //         plugin_id.clone(),
    //         crate::audit::record::OperationType::Rollback,
    //     )
    //     .with_details(serde_json::json!({
    //         "from_version": current_version,
    //         "to_version": target_version,
    //         "duration_ms": start_time.elapsed().as_millis(),
    //     }));
    //     self.audit_logger.log(audit_record).await;
    //
    //     Ok(RollbackResponse {
    //         plugin_id,
    //         from_version: current_version,
    //         to_version: target_version,
    //         success: true,
    //         message: "插件回滚成功".to_string(),
    //     })
    // }
    // ==================== 生命周期操作函数 end ====================

    // ==================== 查询操作 ====================

    /// 获取插件信息
    pub async fn get_plugin(&self, plugin_id: &str) -> PluginResult<Option<PluginInfo>> {
        {
            let registry = self.registry.read().await;
            if let Some(info) = registry.get(plugin_id) {
                return Ok(Some(info.clone()));
            }
        }

        if let Some(record) = self.repository.find_plugin(plugin_id, &self.app_id).await? {
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
                plugin_type: record.plugin_type.unwrap_or_default(),
                source_path: record.source_path,
                app_id: record.app_id,
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
    pub async fn list_plugins(&self, _filter: &PluginFilter) -> PluginResult<Vec<PluginInfo>> {
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

    // ==================== 组件访问器 ====================

    /// 获取数据仓库
    pub fn repository(&self) -> &Arc<PluginRepository> {
        &self.repository
    }

    /// 获取缓存管理器
    pub fn cache(&self) -> &Arc<LayeredCacheManager> {
        &self.cache
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

    /// 获取应用ID
    pub fn app_id(&self) -> &str {
        &self.app_id
    }

    /// 获取运行时加载器
    pub fn runtime_loader(&self) -> &Arc<crate::service::runtime_loader::RuntimeLoader> {
        &self.runtime_loader
    }

    /// 获取管控服务
    pub fn control_service(&self) -> &crate::service::control::ControlService {
        &self.control_service
    }

    /// 关闭插件管理器
    pub async fn shutdown(&self) -> PluginResult<()> {
        let active_plugins = self.activation_manager.get_active_plugins().await;
        for plugin_id in active_plugins {
            let _deactivate_req = DeactivateRequest {
                plugin_id,
                force: true,
            };
            //fixme 暂时注释了
            // let _ = self.deactivate(_deactivate_req).await;
        }

        Ok(())
    }
}
