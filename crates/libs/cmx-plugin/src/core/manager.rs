//! 插件管理器模块
//!
//! 作为核心协调器，协调各子模块完成生命周期操作。
//!
//! # 设计思想
//!
//! PluginManager 是插件系统的核心入口点，负责：
//! - 统一管理所有生命周期服务（安装、卸载、升级、降级、部署）
//! - 协调基础设施组件（数据库、缓存、存储、消息）
//! - 提供插件运行时环境管理
//! - 支持集群模式下的插件管理

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use crate::audit::logger::{AuditLogger, AuditLoggerConfig};
use crate::cluster::node::NodeManager;
use crate::common::{
    DependencyUtils, DependencyUtilsDeps, ServiceUtils, ServiceUtilsDeps,
};
use crate::config::settings::PluginManagerSettings;
use crate::core::context::PluginContext;
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
use crate::service::event_publisher::EventPublisher;
use crate::service::executor::PluginOperationExecutor;
use crate::service::persistence::PluginPersistence;
use crate::service::runtime_ops::{RuntimeOps, RuntimeOpsDeps};
use cmx_buffer::{CacheManager, GlobalCacheManager, GlobalLockManager, LockManager, PubSubOps};
use cmx_database::{DatabaseManager, get_default_db_manager};
use cmx_service::{GlobalServiceQuery, GlobalServiceStorage};
use tokio::sync::RwLock;
use tracing::error;

pub use crate::service::deploy::{DeployAction, DeployRequest, DeployResponse};
pub use crate::service::downgrade::{DowngradeRequest, DowngradeResponse};
pub use crate::service::install::{InstallRequest, InstallResponse};
pub use crate::service::uninstall::{UninstallRequest, UninstallResponse};
pub use crate::service::upgrade::{UpgradeRequest, UpgradeResponse};

/// 插件管理器构建器
///
/// 用于逐步配置和创建 PluginManager 实例。
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
    /// 插件变更通知器（可选）
    plugin_notifier: Option<Arc<crate::cluster::notification::PluginNotifier>>,

    // 服务组件
    /// 安装服务
    install_service: crate::service::install::InstallService,
    /// 升级服务
    upgrade_service: crate::service::upgrade::UpgradeService,
    /// 卸载服务
    uninstall_service: crate::service::uninstall::UninstallService,
    /// 降级服务
    downgrade_service: crate::service::downgrade::DowngradeService,

    /// 部署服务（智能安装/升级）
    deploy_service: crate::service::deploy::DeployService,

    // /// 管控服务（集中式插件管理，不触发本地运行时加载）
    // control_service: crate::service::control::ControlService,

    // 初始化组件
    /// 插件初始化器（用于启动时同步）
    plugin_initializer: crate::service::initializer::PluginInitializer,

    // 新架构组件
    /// 运行时操作层（内存注册/卸载、缓存更新、文件同步）
    runtime_ops: Arc<RuntimeOps>,
    /// 统一事件发布器
    event_publisher: EventPublisher,
    /// 插件操作编排器（统一编排持久化→运行时→事件发布）
    executor: Arc<PluginOperationExecutor>,

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
    pub async fn new(settings: PluginManagerSettings) -> PluginResult<Self> {
        let builder = PluginManagerBuilder::new(settings);
        Self::from_builder(builder).await
    }

    /// 从构建器创建插件管理器
    async fn from_builder(builder: PluginManagerBuilder) -> PluginResult<Self> {
        let settings = builder.settings;

        // 创建插件变更通知器（如果 Redis Pub/Sub 可用）
        let pubsub_for_notifier = builder.pubsub.clone();
        let instance_id = uuid::Uuid::new_v4().to_string();
        let plugin_notifier: Option<Arc<crate::cluster::notification::PluginNotifier>> =
            pubsub_for_notifier.map(|ps| Arc::new(crate::cluster::notification::PluginNotifier::new(ps, instance_id.clone())));

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

        let node_manager =
            if let Some(ref cluster_settings) = settings.cluster {
                Some(Arc::new(NodeManager::new(cluster_settings.node_id.clone())))
            } else {
                None
            };

        let dependency_utils = DependencyUtils::new(DependencyUtilsDeps {
            repository: repository.clone(),
            registry: registry.clone(),
        });

        let service_utils = ServiceUtils::new(ServiceUtilsDeps {
            service_registry: service_registry.clone(),
        });

        // 创建新架构组件
        let event_publisher = EventPublisher::new(plugin_notifier.clone());

        let persistence = PluginPersistence::new(
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
                service_query: GlobalServiceQuery::get().clone(),
                plugin_notifier: plugin_notifier.clone(),
                lock_manager: builder.lock_manager.clone(),
            },
        );

        let runtime_ops = Arc::new(RuntimeOps::new(RuntimeOpsDeps {
            repository: repository.clone(),
            registry: registry.clone(),
            contexts: contexts.clone(),
            cache: cache.clone(),
            plugin_root: settings.plugin_root.clone(),
            temp_root: settings.temp_root.clone(),
            app_id: settings.app_id.clone(),
        }));

        let executor = Arc::new(PluginOperationExecutor::new(
            persistence,
            runtime_ops.clone(),
            event_publisher.clone(),
            audit_logger.clone(),
        ));

        let install_service = crate::service::install::InstallService::new(executor.clone());

        let upgrade_service = crate::service::upgrade::UpgradeService::new(executor.clone());

        let uninstall_service = crate::service::uninstall::UninstallService::new(executor.clone());

        let downgrade_service = crate::service::downgrade::DowngradeService::new(executor.clone());

        let deploy_service =
            crate::service::deploy::DeployService::new(crate::service::deploy::DeployServiceDeps {
                executor: executor.clone(),
                repository: repository.clone(),
                cache: cache.clone(),
                storage: storage.clone(),
                security_validator: security_validator.clone(),
                plugin_root: settings.plugin_root.clone(),
                temp_root: settings.temp_root.clone(),
                app_id: settings.app_id.clone(),
            });

        // let control_service = {
        //     let deps = crate::service::control::ControlServiceDeps {
        //         executor: executor.clone(),
        //         repository: repository.clone(),
        //         app_id: settings.app_id.clone(),
        //     };
        //     let package_utils = crate::common::PackageUtils::new(
        //         crate::common::PackageUtilsDeps {
        //             plugin_root: settings.plugin_root.clone(),
        //             temp_root: settings.temp_root.clone(),
        //             storage: Some(storage.clone()),
        //         },
        //     );
        //     crate::service::control::ControlService::with_package_utils(deps, package_utils)
        // };

        // 创建插件初始化器
        let plugin_initializer = crate::service::initializer::PluginInitializer::new(
            crate::service::initializer::PluginInitializerDeps {
                repository: repository.clone(),
                version_history_repository: version_history_repository.clone(),
                runtime: runtime_ops.clone(),
                event_publisher: event_publisher.clone(),
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
            plugin_notifier,
            install_service,
            upgrade_service,
            uninstall_service,
            downgrade_service,
            deploy_service,
            // control_service,
            plugin_initializer,
            runtime_ops,
            event_publisher,
            executor,
            dependency_utils,
            service_utils,
            initialized: Arc::new(RwLock::new(false)),
        };

        manager.initialize().await?;

        Ok(manager)
    }

    /// 初始化插件管理器
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

        // 加载数据库插件到 context 和 registry
        self.plugin_initializer.load_contexts().await?;

        // 启动 Redis Pub/Sub 订阅，监听跨实例插件变更通知
        if let Some(ref notifier) = self.plugin_notifier {
            let handler = crate::service::plugin_sync::PluginChangeHandler::new(
                self.repository.clone(),
                self.runtime_ops.clone(),
                self.event_publisher.clone(),
                self.settings.plugin_root.clone(),
                self.settings.app_id.clone(),
                notifier.instance_id().to_string(),
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
                self.runtime_ops.clone(),
                self.event_publisher.clone(),
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
        }

        *initialized = true;

        Ok(())
    }

    // ==================== 生命周期操作 ====================

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
    pub async fn deploy(&self, request: DeployRequest) -> PluginResult<DeployResponse> {
        self.deploy_service.deploy(request).await.map_err(|e| {
            error!("部署失败: {}", e);
            e
        })
    }

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

    /// 获取运行时操作层
    pub fn runtime_ops(&self) -> &Arc<RuntimeOps> {
        &self.runtime_ops
    }

    /// 获取统一事件发布器
    pub fn event_publisher(&self) -> &EventPublisher {
        &self.event_publisher
    }

    /// 获取插件操作编排器
    pub fn executor(&self) -> &Arc<PluginOperationExecutor> {
        &self.executor
    }

    /// 获取管控服务
    // pub fn control_service(&self) -> &crate::service::control::ControlService {
    //     &self.control_service
    // }

    /// 关闭插件管理器
    pub async fn shutdown(&self) -> PluginResult<()> {
        let _active_plugins = self.activation_manager.get_active_plugins().await;
        tracing::info!("插件管理器关闭");
        Ok(())
    }
}
