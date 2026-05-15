/*
 * @Author: yqs
 * @Date: 2026-03-10 15:35:34
 * @Describe:
 * @LastEditors: yqs
 * @LastEditTime: 2026-03-17 19:59:28
 */
//! Web 服务器配置模块
//!
//! 提供应用程序初始化功能，包括：
//! - 全局配置加载
//! - 数据库数据源初始化
//! - Redis 缓存初始化
//! - WASM 运行时初始化
//! - 插件管理器初始化
//! - 服务管理器初始化

use cmx_buffer::{GlobalCacheManager, GlobalLockManager, RedisClient, RedisConfig};
use cmx_database::get_default_db_manager;
use cmx_nacos::{GlobalConfigChangeNotifier, NacosClient, NacosConfig, RemoteConfigChangeListener};
use cmx_utils::{ ConfigBuilder, ConfigManager, ConfigResult};
use serde::Deserialize;
use std::sync::{Arc, OnceLock};
use tracing::{error, info, warn};

pub use crate::datasource_init::init_datasources;

/// 全局 NacosClient 存储
///
/// 使用 OnceLock 存储 NacosClient 实例，供后续服务注册和配置监听使用
static GLOBAL_NACOS_CLIENT: OnceLock<NacosClient> = OnceLock::new();

/// 存储 NacosClient 到全局静态变量
///
/// # 参数
/// - `client`: NacosClient 实例
fn store_nacos_client(client: NacosClient) {
    let _ = GLOBAL_NACOS_CLIENT.set(client);
}

/// 获取全局 NacosClient 引用
///
/// # 返回值
/// 如果已初始化返回 Some(引用)，否则返回 None
#[allow(dead_code)]
pub fn get_nacos_client() -> Option<&'static NacosClient> {
    GLOBAL_NACOS_CLIENT.get()
}

// /// 初始化全局配置（传统方式，不含 Nacos）
// #[allow(dead_code)]
// pub fn init_global_config() {
//     info!("加载环境变量和配置文件信息...");
//     ConfigManager::initialize(|| {
//         ConfigBuilder::new()
//             .add_toml_file_from_env("CONFIG_FILE")
//             .add_env()
//             // .add_source(CommandLineSource::from_args(std::env::args().skip(1)))
//             .build()
//     })
//     .unwrap();
//     info!("打印所有配置和环境变量键值对...");
//     for key in ConfigManager::global().keys() {
//         if "Path" == key {
//             continue;
//         }
//         info!("{:?}: {:?}", key, ConfigManager::global().get(&key));
//     }
// }

/// 初始化全局配置（含 Nacos 远程配置覆盖）
///
/// 配置加载流程：
/// 1. 从环境变量（NACOS_* 前缀）读取 Nacos 连接信息
/// 2. 先用本地 TOML 构建初始 Config
/// 3. 若 Nacos 启用且配置中心启用，初始化 NacosClient 并拉取远程配置
/// 4. 重新构建 Config（本地 TOML + 远程配置（过滤 nacos/migration） + 环境变量）
/// 5. 更新 ConfigManager 全局单例
///
/// 配置优先级（从高到低）：
/// - 环境变量（add_env 最后添加，优先级最高）> 远程配置 > 本地 TOML > 代码默认值
///
/// 安全保障：
/// - Nacos 连接信息从环境变量读取，不受远程配置影响
/// - 远程配置自动过滤 `nacos` 和 `migration` 段，防止启动参数被覆盖
/// - 环境变量始终是最高优先级，不可被远程配置覆盖
pub async fn init_global_config_with_nacos() {
    info!("加载环境变量和配置文件信息...");

    // 步骤1：从环境变量读取 Nacos 配置（不从 TOML 读取，避免被远程配置覆盖）
    let nacos_config = NacosConfig::from_env();

    if !nacos_config.enabled {
        info!("Nacos 未启用（NACOS_ENABLED 未设置或为 false），使用本地配置");
        init_global_config_fallback();
        return;
    }

    // 步骤2：先用本地 TOML 构建初始 Config
    let _initial_config = match ConfigBuilder::new()
        .add_toml_file_from_env("CONFIG_FILE")
        .build()
    {
        Ok(config) => config,
        Err(e) => {
            panic!("初始配置加载失败: {:?}", e);
        }
    };

    // 步骤3：初始化 NacosClient
    match NacosClient::new(nacos_config.clone()) {
        Ok(client) => {
            // 步骤4：拉取远程配置并注入 ConfigBuilder
            let mut builder = ConfigBuilder::new()
                .add_toml_file_from_env("CONFIG_FILE");

            if nacos_config.config.enabled
                && let Some(listener) = nacos_config.config.listeners.first()
            {
                match client.get_config_source(&listener.data_id, &listener.group).await {
                    Ok(source) => {
                        info!(
                            "成功从 Nacos 拉取远程配置: {}/{}",
                            listener.group, listener.data_id
                        );
                        builder = builder.add_source(source);
                    }
                    Err(e) => {
                        warn!("从 Nacos 拉取远程配置失败: {}，使用本地配置", e);
                    }
                }
            }

            // 环境变量最后添加，确保最高优先级
            builder = builder.add_env();
            let final_config = builder.build().expect("配置构建失败");
            ConfigManager::initialize(|| Ok::<_, cmx_utils::ConfigError>(final_config)).unwrap();

            // 步骤5：存储 NacosClient 供后续使用
            store_nacos_client(client);
            info!("配置初始化完成（含 Nacos 远程配置覆盖）");

            // 步骤6：注册服务到 Nacos 命名服务
            if let Some(client) = get_nacos_client() {
                register_nacos_service(client).await;
            }

            // 步骤7：初始化配置变更通知器并注册监听
            if let Some(client) = get_nacos_client() {
                setup_config_listener(client, &nacos_config).await;
            }
        }
        Err(e) => {
            warn!("Nacos 客户端初始化失败: {}，回退到本地配置", e);
            init_global_config_fallback();
        }
    }

    // 打印所有配置和环境变量键值对
    for key in ConfigManager::global().keys() {
        if "Path" == key {
            continue;
        }
        info!("{:?}: {:?}", key, ConfigManager::global().get(&key));
    }
}

/// 回退到本地配置加载
fn init_global_config_fallback() {
    ConfigManager::initialize(|| {
        ConfigBuilder::new()
            .add_toml_file_from_env("CONFIG_FILE")
            .add_env()
            .build()
    })
    .unwrap();
}

/// 注册服务到 Nacos 命名服务
///
/// 从配置中读取服务端口，将当前服务实例注册到 Nacos。
/// 注册失败时仅记录警告，不阻止启动。
async fn register_nacos_service(client: &NacosClient) {
    if !client.is_naming_enabled() {
        info!("Nacos 命名服务未启用，跳过服务注册");
        return;
    }

    let port: u16 = ConfigManager::global()
        .get_string("server.port")
        .unwrap_or_else(|_| "8080".to_string())
        .parse()
        .unwrap_or(8080);

    let ip = local_ip_address::local_ip()
        .map(|addr| addr.to_string())
        .unwrap_or_else(|_| "127.0.0.1".to_string());

    match client.register_service(&ip, port).await {
        Ok(_) => {
            info!("服务实例已注册到 Nacos: {}:{}", ip, port);
        }
        Err(e) => {
            warn!("服务注册到 Nacos 失败: {}，服务仍可正常运行", e);
        }
    }
}

/// 设置配置变更监听器
///
/// 初始化全局配置变更通知器，并为每个配置监听项注册 RemoteConfigChangeListener。
/// 监听失败时仅记录警告，不影响启动。
async fn setup_config_listener(client: &NacosClient, nacos_config: &NacosConfig) {
    if !client.is_config_enabled() {
        info!("Nacos 配置中心未启用，跳过配置监听注册");
        return;
    }

    // 初始化全局配置变更通知器
    GlobalConfigChangeNotifier::initialize();

    let listener = Arc::new(RemoteConfigChangeListener);

    for config_listener in &nacos_config.config.listeners {
        match client
            .listen_config(&config_listener.data_id, &config_listener.group, listener.clone())
            .await
        {
            Ok(_) => {
                info!(
                    "已注册 Nacos 配置变更监听: {}/{}",
                    config_listener.group, config_listener.data_id
                );
            }
            Err(e) => {
                warn!(
                    "注册 Nacos 配置变更监听失败 [{}/{}]: {}",
                    config_listener.group, config_listener.data_id, e
                );
            }
        }
    }
}

/// 优雅关闭：从 Nacos 注销服务实例
///
/// 在应用关闭时调用，确保服务实例从 Nacos 命名服务中注销，
/// 避免其他服务发现已下线的实例。
pub async fn shutdown_nacos() {
    if let Some(client) = get_nacos_client() {
        if !client.is_naming_enabled() {
            return;
        }

        let port: u16 = ConfigManager::global()
            .get_string("server.port")
            .unwrap_or_else(|_| "8080".to_string())
            .parse()
            .unwrap_or(8080);

        let ip = local_ip_address::local_ip()
            .map(|addr| addr.to_string())
            .unwrap_or_else(|_| "127.0.0.1".to_string());

        match client.deregister_service(&ip, port).await {
            Ok(_) => {
                info!("服务实例已从 Nacos 注销: {}:{}", ip, port);
            }
            Err(e) => {
                warn!("服务从 Nacos 注销失败: {}", e);
            }
        }
    }
}

/// 获取 Web 配置单例
///
/// 返回一个静态的 WebConfig 引用，确保配置只加载一次
///
/// # 返回值
/// - `&'static WebConfig` - Web 服务器配置引用
pub fn web_config() -> &'static WebConfig {
    static INSTANCE: OnceLock<WebConfig> = OnceLock::new();

    INSTANCE.get_or_init(|| {
        WebConfig::load_from_env()
            .unwrap_or_else(|ex| panic!("FATAL - WHILE LOADING CONF - Cause: {ex:?}"))
    })
}

/// Web 服务器配置结构
#[derive(Debug, Deserialize)]
pub struct WebConfig {
    /// Web 静态文件目录
    pub web_folder: String,
}

impl WebConfig {
    /// 从环境变量加载配置
    ///
    /// # 返回值
    fn load_from_env() -> ConfigResult<WebConfig> {
        let result = ConfigManager::global().get_string("web_folder");

        match result {
            Ok(value) => Ok(WebConfig { web_folder: value }),
            Err(ex) => Err(ex),
        }
    }
}

/// 初始化数据库迁移
///
/// 在应用启动时自动执行待执行的数据库迁移，
/// 支持分布式锁防止多节点并发执行
pub async fn init_database_migrations() {
    use cmx_database::migration::MigrationRunner;
    use cmx_buffer::GlobalLockManager;
    use std::path::PathBuf;

    let db_manager = get_default_db_manager();
    let default_db_id = db_manager.get_default_db_id().await;
    let migration_dir = ConfigManager::global()
        .get_string("migration.dir")
        .unwrap_or("docs/sql/migrations".to_string());
    let node_id = ConfigManager::global()
        .get_string("node.node_id")
        .unwrap_or("default".to_string());

    let runner = MigrationRunner::new(
        db_manager.clone(),
        default_db_id,
        PathBuf::from(migration_dir),
        node_id,
    );

    let runner = if GlobalLockManager::is_initialized() {
        runner.with_lock_manager(GlobalLockManager::get().clone())
    } else {
        runner
    };

    let validate_checksum = ConfigManager::global()
        .get_bool("migration.validate_checksum")
        .unwrap_or(true);
    let runner = runner.with_validate_checksum(validate_checksum);

    match runner.run_pending_migrations().await {
        Ok(summary) => {
            info!(
                "数据库迁移完成: 执行={}, 跳过={}, 失败={}",
                summary.executed_count,
                summary.skipped_count,
                summary.failed.len()
            );
            if !summary.failed.is_empty() {
                panic!("数据库迁移存在失败项，终止启动");
            }
        }
        Err(e) => {
            panic!("数据库迁移执行失败: {:?}", e);
        }
    }
}

/// 初始化缓存
///
/// 创建唯一的 RedisClient，由 GlobalCacheManager 和 GlobalLockManager 共享，
/// 避免创建多个独立的 bb8 连接池。
pub async fn init_cache() {
    let config = ConfigManager::global();
    let _url_value = match config.get("redis.url") {
        Some(url_value) => url_value,
        None => {
            error!("无法从配置管理器获取 redis 配置");
            panic!("无法获取redis配置");
        }
    };

    let redis_config = config.get_as::<RedisConfig>("redis").unwrap();

    // 创建唯一的 RedisClient 实例，共享给 CacheManager 和 LockManager
    let client = RedisClient::new(redis_config)
        .await
        .expect("Redis 客户端创建失败");

    GlobalCacheManager::initialize_with_client(client.clone())
        .expect("redis初始化失败");
    info!("redis缓存初始化完成");

    GlobalLockManager::initialize_with_client(client)
        .expect("redis分布式锁初始化失败");
    info!("redis分布式锁初始化完成");
}

/// 初始化 WASM 运行时
///
/// 必须在 init_db_datasource 和 init_cache 之后调用。
/// 注册所有宿主函数提供者到 WASM 引擎。
pub async fn init_runtime() {
    use cmx_runtime::{ExtismEngine, ExtismEngineConfig, GlobalExtismEngine};
    use cmx_traits::{GlobalRuntime, HostFunctionProvider};
    use cmx_utils::LoggingHostFunctions;
    use cmx_database::DatabaseHostFunctions;
    use cmx_buffer::BufferHostFunctions;
    use cmx_plugin::PluginHostFunctions;

    info!("初始化 WASM 运行时...");

    // 创建 Extism 引擎
    let engine = Arc::new(
        ExtismEngine::new(ExtismEngineConfig::default())
            .expect("Extism 引擎初始化失败")
    );

    // 注册宿主函数提供者
    // 1. 日志宿主函数
    let logging_provider: Arc<dyn HostFunctionProvider> = Arc::new(LoggingHostFunctions::new());
    engine.register_provider(logging_provider)
        .expect("注册日志宿主函数失败");

    // 2. 数据库宿主函数
    let db_provider: Arc<dyn HostFunctionProvider> = Arc::new(
        DatabaseHostFunctions::new(cmx_database::get_default_db_manager().clone())
    );
    engine.register_provider(db_provider)
        .expect("注册数据库宿主函数失败");

    // 3. 缓存宿主函数
    let buffer_provider: Arc<dyn HostFunctionProvider> = Arc::new(BufferHostFunctions::new());
    engine.register_provider(buffer_provider)
        .expect("注册缓存宿主函数失败");

    // 4. 插件间调用宿主函数
    let plugin_provider: Arc<dyn HostFunctionProvider> = Arc::new(PluginHostFunctions::new());
    engine.register_provider(plugin_provider)
        .expect("注册插件宿主函数失败");

    // 设置全局运行时（供 PluginHostFunctions 使用）
    GlobalRuntime::set(engine.clone())
        .expect("设置全局运行时失败");

    // 初始化全局引擎
    GlobalExtismEngine::initialize(engine)
        .expect("全局引擎初始化失败");

    info!("WASM 运行时初始化完成，已注册 4 个宿主函数提供者");
}

/// 初始化插件管理器
///
/// 必须在 init_runtime 之后调用，因为需要注册 PluginHostFunctions。
pub async fn init_plugins() {
    use cmx_plugin::{GlobalPluginManager, PluginManagerSettings};
    use std::path::PathBuf;

    info!("初始化插件管理器...");

    let default_db_id = get_default_db_manager().get_default_db_id().await;
    let plugin_root = ConfigManager::global().get_string("plugin.install_root").unwrap_or("plugins/root".to_string());
    let backup_root = ConfigManager::global().get_string("plugin.backup_root").unwrap_or("plugins/backup".to_string());
    let temp_root = ConfigManager::global().get_string("plugin.temp_root").unwrap_or("plugins/temp".to_string());

    // 加载自动安装配置
    let auto_install_config = ConfigManager::global()
        .get_as::<cmx_plugin::AutoInstallConfig>("plugin.auto_install")
        .unwrap_or_default();

    let settings = PluginManagerSettings {
        plugin_root: PathBuf::from(plugin_root),
        backup_root: PathBuf::from(backup_root),
        temp_root: PathBuf::from(temp_root),
        default_database_id: default_db_id,
        node_id: ConfigManager::global().get_string("node.node_id").ok(),
        auto_install: auto_install_config,
        ..Default::default()
    };

    GlobalPluginManager::initialize(settings)
        .await
        .unwrap_or_else(|e| panic!("初始化插件管理器失败: {:?}", e));
    info!("成功初始化插件管理器");
}

/// 初始化服务管理器
///
/// 初始化服务相关的组件，服务数据采用延迟加载策略：
/// - 不在启动时全量加载所有服务
/// - 首次访问服务时才从数据库加载并缓存
///
/// 这样做的好处：
/// - 加快服务启动速度，特别是服务数量较多时
/// - 避免启动时执行大量数据库查询（1+2N 次）
pub async fn init_services() {
    use cmx_service::{GlobalServiceQuery, GlobalServiceStorage, GlobalServiceRegistry, ServiceRepository, ServiceRegistry, ServiceQueryImpl, ServiceStorageImpl, ServiceLifecycleListener};
    use cmx_runtime::RuntimeLifecycleListener;
    use cmx_traits::{ServiceQuery, ServiceStorage};

    info!("初始化服务管理器...");

    let db_manager = get_default_db_manager();
    let default_db_id = get_default_db_manager().get_default_db_id().await;

    let repository = Arc::new(ServiceRepository::new(db_manager.clone(), default_db_id));
    let registry = Arc::new(ServiceRegistry::new());

    GlobalServiceRegistry::set(registry.clone()).expect("初始化服务注册中心失败");

    let service_query = Arc::new(ServiceQueryImpl::new(repository.clone(), registry.clone())) as Arc<dyn ServiceQuery>;
    let service_storage = Arc::new(ServiceStorageImpl::new(repository.clone())) as Arc<dyn ServiceStorage>;

    GlobalServiceQuery::set(service_query.clone()).expect("初始化服务查询器失败");
    GlobalServiceStorage::set(service_storage.clone()).expect("初始化服务存储失败");

    info!("服务仓储使用数据库ID: {}", repository.get_default_db_id());
    info!("服务数据采用延迟加载策略，首次访问时自动加载");

    // 注册服务生命周期监听器
    let service_listener = ServiceLifecycleListener::new(
        GlobalServiceQuery::get().clone(),
        repository.clone(),
        GlobalServiceRegistry::get().clone(),
    );
    service_listener.register().await;

    // 注册运行时生命周期监听器
    let runtime_listener = RuntimeLifecycleListener::new(
        cmx_runtime::GlobalExtismEngine::get_as_invoker()
    );
    runtime_listener.register().await;

    info!("生命周期监听器已注册");
    info!("服务管理器初始化完成");
}

/// 初始化文件存储服务
///
/// 从全局配置中加载存储配置，创建 `StorageManager` 和 `DefaultStorageService`，
/// 并注册到 `GlobalStorageService` 全局单例。
///
/// 必须在 `init_datasources` 之后调用，因为存储服务依赖数据库进行文件元信息管理。
pub async fn init_storage() {
    use cmx_storage::config::StorageManagerConfig;
    use cmx_storage::global::GlobalStorageService;
    use cmx_storage::manager::StorageManager;
    use cmx_storage::service::DefaultStorageService;

    info!("初始化文件存储服务...");

    let config = ConfigManager::global();
    let storage_config = StorageManagerConfig::from_config(config)
        .expect("存储配置加载失败");

    let manager = Arc::new(
        StorageManager::new(&storage_config).expect("存储管理器初始化失败"),
    );

    let service: Arc<dyn cmx_storage::service::StorageService> =
        Arc::new(DefaultStorageService::new(manager));

    GlobalStorageService::initialize(service).expect("存储服务全局初始化失败");

    info!("文件存储服务初始化完成");
}
