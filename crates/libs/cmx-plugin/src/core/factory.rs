// //! 插件管理器工厂模块
// //!
// //! 将 PluginManager 的复杂初始化逻辑拆分为多个职责单一的工厂方法，
// //! 每个方法只负责创建一组相关的组件，降低 from_builder 方法的复杂度。
//
// use std::collections::HashMap;
// use std::sync::Arc;
//
// use crate::audit::logger::{AuditLogger, AuditLoggerConfig};
// use crate::cluster::node::NodeManager;
// use crate::common::{DependencyUtils, DependencyUtilsDeps, ServiceUtils, ServiceUtilsDeps};
// use crate::config::settings::PluginManagerSettings;
// use crate::core::context::PluginContext;
// use crate::core::manager::PluginManager;
// use crate::core::registry::PluginRegistry;
// use crate::error::PluginResult;
// use crate::infrastructure::cache::layered::LayeredCacheManager;
// use crate::infrastructure::database::repository::PluginRepository;
// use crate::infrastructure::database::version_history::VersionHistoryRepository;
// use crate::infrastructure::storage::backup::BackupManager;
// use crate::infrastructure::storage::file::FileStorage;
// use crate::runtime::activation::ActivationManager;
// use crate::runtime::service_registry::ServiceRegistry;
// use crate::security::validator::SecurityValidator;
// use crate::service::event_publisher::EventPublisher;
// use crate::service::executor::PluginOperationExecutor;
// use crate::service::persistence::PluginPersistence;
// use crate::service::runtime_ops::{RuntimeOps, RuntimeOpsDeps};
// use cmx_buffer::{CacheManager, LockManager, PubSubOps};
// use cmx_database::DatabaseManager;
// use cmx_service::{GlobalServiceQuery, GlobalServiceStorage};
// use tokio::sync::RwLock;
//
// // ==================== 组件组结构体 ====================
//
// /// 基础设施组件组
// struct InfrastructureComponents {
//     repository: Arc<PluginRepository>,
//     version_history_repository: Arc<VersionHistoryRepository>,
//     cache: Arc<LayeredCacheManager>,
//     storage: Arc<FileStorage>,
//     backup_manager: Arc<BackupManager>,
// }
//
// /// 安全组件组
// struct SecurityComponents {
//     security_validator: Arc<SecurityValidator>,
// }
//
// /// 运行时组件组
// struct RuntimeComponents {
//     activation_manager: Arc<ActivationManager>,
//     service_registry: Arc<ServiceRegistry>,
//     registry: Arc<RwLock<PluginRegistry>>,
//     contexts: Arc<RwLock<HashMap<String, PluginContext>>>,
// }
//
// /// 审计组件组
// struct AuditComponents {
//     audit_logger: Arc<AuditLogger>,
// }
//
// /// 集群组件组
// struct ClusterComponents {
//     node_manager: Option<Arc<NodeManager>>,
//     plugin_notifier: Option<Arc<crate::cluster::notification::PluginNotifier>>,
// }
//
// /// 服务组件组
// struct ServiceComponents {
//     install_service: crate::service::install::InstallService,
//     upgrade_service: crate::service::upgrade::UpgradeService,
//     uninstall_service: crate::service::uninstall::UninstallService,
//     downgrade_service: crate::service::downgrade::DowngradeService,
//     deploy_service: crate::service::deploy::DeployService,
// }
//
// /// 工具组件组
// struct UtilityComponents {
//     dependency_utils: DependencyUtils,
//     service_utils: ServiceUtils,
// }
//
// /// 新架构组件组
// struct NewArchitectureComponents {
//     event_publisher: EventPublisher,
//     runtime_ops: Arc<RuntimeOps>,
//     executor: Arc<PluginOperationExecutor>,
// }
//
// // ==================== 工厂 ====================
//
// /// 插件管理器工厂
// ///
// /// 将 PluginManager 的复杂初始化逻辑按职责拆分为多个工厂方法，
// /// 每个方法只负责创建一组相关的组件。
// pub(crate) struct PluginManagerFactory {
//     pub(crate) settings: PluginManagerSettings,
//     pub(crate) db_manager: Arc<DatabaseManager>,
//     pub(crate) lock_manager: Option<Arc<LockManager>>,
//     pub(crate) pubsub: Option<Arc<PubSubOps>>,
//     #[allow(dead_code)]
//     pub(crate) cache_manager: Option<Arc<CacheManager>>,
// }
//
// impl PluginManagerFactory {
//     /// 创建基础设施组件
//     fn create_infrastructure_components(&self) -> InfrastructureComponents {
//         let repository = Arc::new(PluginRepository::new(
//             self.db_manager.clone(),
//             self.settings.default_database_id.clone(),
//         ));
//
//         let version_history_repository = Arc::new(VersionHistoryRepository::new(
//             self.db_manager.clone(),
//             self.settings.default_database_id.clone(),
//         ));
//
//         let cache = Arc::new(LayeredCacheManager::new(Default::default()));
//         let storage = Arc::new(FileStorage::new(&self.settings.plugin_root));
//         let backup_manager = Arc::new(BackupManager::new(self.settings.backup_root.clone()));
//
//         InfrastructureComponents {
//             repository,
//             version_history_repository,
//             cache,
//             storage,
//             backup_manager,
//         }
//     }
//
//     /// 创建安全组件
//     fn create_security_components(&self) -> SecurityComponents {
//         SecurityComponents {
//             security_validator: Arc::new(SecurityValidator::new()),
//         }
//     }
//
//     /// 创建运行时组件
//     fn create_runtime_components(&self) -> RuntimeComponents {
//         RuntimeComponents {
//             activation_manager: Arc::new(ActivationManager::new()),
//             service_registry: Arc::new(ServiceRegistry::new()),
//             registry: Arc::new(RwLock::new(PluginRegistry::new())),
//             contexts: Arc::new(RwLock::new(HashMap::new())),
//         }
//     }
//
//     /// 创建审计组件
//     fn create_audit_components(&self) -> AuditComponents {
//         let audit_logger_config = AuditLoggerConfig::new(
//             self.db_manager.clone(),
//             self.settings.default_database_id.clone(),
//             self.settings
//                 .node_id
//                 .clone()
//                 .unwrap_or_else(|| "default".to_string()),
//         );
//         AuditComponents {
//             audit_logger: Arc::new(AuditLogger::new(audit_logger_config)),
//         }
//     }
//
//     /// 创建集群组件
//     fn create_cluster_components(&self) -> ClusterComponents {
//         let plugin_notifier = self.pubsub.as_ref().map(|ps| {
//             let instance_id = uuid::Uuid::new_v4().to_string();
//             Arc::new(crate::cluster::notification::PluginNotifier::new(
//                 ps.clone(),
//                 instance_id,
//             ))
//         });
//
//         let node_manager = self.settings.cluster.as_ref().map(|cluster_settings| {
//             Arc::new(NodeManager::new(cluster_settings.node_id.clone()))
//         });
//
//         ClusterComponents {
//             node_manager,
//             plugin_notifier,
//         }
//     }
//
//     /// 创建工具组件
//     fn create_utility_components(
//         &self,
//         infrastructure: &InfrastructureComponents,
//         runtime: &RuntimeComponents,
//     ) -> UtilityComponents {
//         let dependency_utils = DependencyUtils::new(DependencyUtilsDeps {
//             repository: infrastructure.repository.clone(),
//             registry: runtime.registry.clone(),
//         });
//
//         let service_utils = ServiceUtils::new(ServiceUtilsDeps {
//             service_registry: runtime.service_registry.clone(),
//         });
//
//         UtilityComponents {
//             dependency_utils,
//             service_utils,
//         }
//     }
//
//     /// 创建新架构组件
//     fn create_new_architecture_components(
//         &self,
//         infrastructure: &InfrastructureComponents,
//         runtime: &RuntimeComponents,
//         audit: &AuditComponents,
//         cluster: &ClusterComponents,
//         security: &SecurityComponents,
//     ) -> NewArchitectureComponents {
//         let event_publisher = EventPublisher::new(cluster.plugin_notifier.clone());
//
//         let persistence = PluginPersistence::new(
//             crate::service::install::InstallServiceDeps {
//                 repository: infrastructure.repository.clone(),
//                 version_history_repository: infrastructure.version_history_repository.clone(),
//                 cache: infrastructure.cache.clone(),
//                 storage: infrastructure.storage.clone(),
//                 backup_manager: infrastructure.backup_manager.clone(),
//                 security_validator: security.security_validator.clone(),
//                 audit_logger: audit.audit_logger.clone(),
//                 registry: runtime.registry.clone(),
//                 contexts: runtime.contexts.clone(),
//                 plugin_root: self.settings.plugin_root.clone(),
//                 temp_root: self.settings.temp_root.clone(),
//                 default_database_id: self.settings.default_database_id.clone(),
//                 node_name: self.settings.node_name.clone(),
//                 node_type: self.settings.node_type.clone(),
//                 service_storage: GlobalServiceStorage::get().clone(),
//                 service_query: GlobalServiceQuery::get().clone(),
//                 plugin_notifier: cluster.plugin_notifier.clone(),
//                 lock_manager: self.lock_manager.clone(),
//             },
//         );
//
//         let runtime_ops = Arc::new(RuntimeOps::new(RuntimeOpsDeps {
//             repository: infrastructure.repository.clone(),
//             registry: runtime.registry.clone(),
//             contexts: runtime.contexts.clone(),
//             cache: infrastructure.cache.clone(),
//             plugin_root: self.settings.plugin_root.clone(),
//             temp_root: self.settings.temp_root.clone(),
//             app_id: self.settings.app_id.clone(),
//         }));
//
//         let executor = Arc::new(PluginOperationExecutor::new(
//             persistence,
//             runtime_ops.clone(),
//             event_publisher.clone(),
//             audit.audit_logger.clone(),
//         ));
//
//         NewArchitectureComponents {
//             event_publisher,
//             runtime_ops,
//             executor,
//         }
//     }
//
//     /// 创建服务组件
//     fn create_service_components(
//         &self,
//         infrastructure: &InfrastructureComponents,
//         security: &SecurityComponents,
//         new_arch: &NewArchitectureComponents,
//     ) -> ServiceComponents {
//         let install_service = crate::service::install::InstallService::new(new_arch.executor.clone());
//         let upgrade_service = crate::service::upgrade::UpgradeService::new(new_arch.executor.clone());
//         let uninstall_service = crate::service::uninstall::UninstallService::new(new_arch.executor.clone());
//         let downgrade_service = crate::service::downgrade::DowngradeService::new(new_arch.executor.clone());
//
//         let deploy_service =
//             crate::service::deploy::DeployService::new(crate::service::deploy::DeployServiceDeps {
//                 executor: new_arch.executor.clone(),
//                 repository: infrastructure.repository.clone(),
//                 cache: infrastructure.cache.clone(),
//                 storage: infrastructure.storage.clone(),
//                 security_validator: security.security_validator.clone(),
//                 plugin_root: self.settings.plugin_root.clone(),
//                 temp_root: self.settings.temp_root.clone(),
//                 app_id: self.settings.app_id.clone(),
//             });
//
//         ServiceComponents {
//             install_service,
//             upgrade_service,
//             uninstall_service,
//             downgrade_service,
//             deploy_service,
//         }
//     }
//
//     /// 创建插件初始化器
//     fn create_plugin_initializer(
//         &self,
//         infrastructure: &InfrastructureComponents,
//         new_arch: &NewArchitectureComponents,
//     ) -> crate::service::initializer::PluginInitializer {
//         crate::service::initializer::PluginInitializer::new(
//             crate::service::initializer::PluginInitializerDeps {
//                 repository: infrastructure.repository.clone(),
//                 version_history_repository: infrastructure.version_history_repository.clone(),
//                 runtime: new_arch.runtime_ops.clone(),
//                 event_publisher: new_arch.event_publisher.clone(),
//                 plugin_root: self.settings.plugin_root.clone(),
//                 app_id: self.settings.app_id.clone(),
//             },
//         )
//     }
//
//     /// 构建完整的 PluginManager
//     pub async fn build(self) -> PluginResult<PluginManager> {
//         // 按依赖顺序创建各组件组
//         let infrastructure = self.create_infrastructure_components();
//         let security = self.create_security_components();
//         let runtime = self.create_runtime_components();
//         let audit = self.create_audit_components();
//         let cluster = self.create_cluster_components();
//         let utilities = self.create_utility_components(&infrastructure, &runtime);
//         let new_arch = self.create_new_architecture_components(
//             &infrastructure,
//             &runtime,
//             &audit,
//             &cluster,
//             &security,
//         );
//         let services = self.create_service_components(&infrastructure, &security, &new_arch);
//         let plugin_initializer = self.create_plugin_initializer(&infrastructure, &new_arch);
//
//         // 组装最终的 PluginManager
//         let manager = PluginManager::from_components(
//             self.settings,
//             runtime.registry,
//             runtime.contexts,
//             infrastructure.repository,
//             infrastructure.cache,
//             infrastructure.storage,
//             infrastructure.backup_manager,
//             security.security_validator,
//             runtime.activation_manager,
//             runtime.service_registry,
//             audit.audit_logger,
//             cluster.node_manager,
//             cluster.plugin_notifier,
//             services.install_service,
//             services.upgrade_service,
//             services.uninstall_service,
//             services.downgrade_service,
//             services.deploy_service,
//             plugin_initializer,
//             new_arch.runtime_ops,
//             new_arch.event_publisher,
//             new_arch.executor,
//             utilities.dependency_utils,
//             utilities.service_utils,
//         );
//
//         manager.initialize().await?;
//
//         Ok(manager)
//     }
// }
