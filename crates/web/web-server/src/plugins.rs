// use std::path::PathBuf;
// use std::sync::Arc;
//
// use cmx_buffer::{GlobalCacheManager, LockManager};
// use cmx_database::get_default_db_manager;
// use cmx_plugin::{
//     ActivationManager, AuditLogger, CmxPluginDatabase, DeploymentCoordinator,
//     MessageQueue, MessageQueueBuilder, NodeManager, NodeManagerConfig, NodeSelectionStrategy,
//     PermissionChecker, PermissionPolicy, PluginCacheManager, PluginManager,
//     PluginManagerConfig, SecurityValidator, SecurityValidatorConfig, ServiceRegistry,
// };
//
// pub struct PluginSystem {
//     pub manager: Arc<PluginManager>,
//     pub node_manager: Arc<NodeManager>,
//     pub message_queue: Arc<MessageQueue>,
//     pub service_registry: Arc<ServiceRegistry>,
// }
//
// impl PluginSystem {
//     pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
//         let config = PluginManagerConfig {
//             install_root: PathBuf::from("./plugins/prod"),
//             temp_root: PathBuf::from("./plugins/temp"),
//             backup_root: PathBuf::from("./plugins/backups"),
//             max_concurrent_installs: 1,
//             install_timeout_seconds: 300,
//             upgrade_timeout_seconds: 300,
//             require_signature: false,
//             trusted_signing_keys: vec![],
//             default_plugins: vec![],
//             default_db_id: "default".to_string(),
//         };
//
//         let cache_manager = GlobalCacheManager::get_cloned();
//         let db_manager = get_default_db_manager();
//
//         let redis_client = cache_manager.client().clone();
//         let lock_manager = LockManager::new_with_default_config(redis_client);
//
//         let audit_logger = Arc::new(AuditLogger::new());
//
//         let security_validator = Arc::new(SecurityValidator::new(
//             SecurityValidatorConfig {
//                 require_signature: false,
//                 trusted_public_keys: vec![],
//                 verify_file_hash: true,
//                 max_plugin_size: 100 * 1024 * 1024,
//                 enable_sandbox: true,
//                 allowed_imports: vec!["env".to_string()],
//             }
//         ));
//
//         let plugin_cache_manager = Arc::new(PluginCacheManager::new(
//             cache_manager.clone(),
//             lock_manager.clone(),
//         ));
//
//         let deployment_coordinator = Arc::new(
//             DeploymentCoordinator::with_lock_manager(lock_manager)
//         );
//
//         let activation_manager = Arc::new(ActivationManager::new());
//
//         let db_service = Arc::new(CmxPluginDatabase::new((**db_manager).clone()));
//
//         let node_manager = Arc::new(NodeManager::new(NodeManagerConfig {
//             heartbeat_timeout_seconds: 30,
//             health_check_interval_seconds: 10,
//             selection_strategy: NodeSelectionStrategy::RoundRobin,
//         }));
//
//         let message_queue = Arc::new(
//             MessageQueueBuilder::new()
//                 .enabled(true)
//                 .redis_url("redis://127.0.0.1:6379")
//                 .build()
//         );
//
//         let service_registry = Arc::new(ServiceRegistry::new());
//
//         let permission_checker = Arc::new(PermissionChecker::new(PermissionPolicy::Strict));
//
//         let manager = Arc::new(
//             PluginManager::with_components(
//                 config,
//                 Some(activation_manager),
//                 Some(deployment_coordinator),
//                 Some(plugin_cache_manager),
//                 Some(db_service),
//             )?
//         );
//
//         Ok(Self {
//             manager,
//             node_manager,
//             message_queue,
//             service_registry,
//         })
//     }
// }
//
// pub async fn init_plugins() -> Result<PluginSystem, Box<dyn std::error::Error>> {
//     PluginSystem::new().await
// }
