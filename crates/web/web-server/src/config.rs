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

use cmx_buffer::{GlobalCacheManager, GlobalLockManager, RedisConfig};
use cmx_database::get_default_db_manager;
use cmx_utils::{ ConfigBuilder, ConfigManager, ConfigResult};
use serde::Deserialize;
use std::sync::{Arc, OnceLock};
use tracing::{error, info};

pub use crate::datasource_init::init_datasources;

pub fn init_global_config() {
    info!("加载环境变量和配置文件信息...");
    ConfigManager::initialize(|| {
        ConfigBuilder::new()
            .add_toml_file_from_env("CONFIG_FILE")
            .add_env()
            // .add_source(CommandLineSource::from_args(std::env::args().skip(1)))
            .build()
    })
    .unwrap();
    info!("打印所有配置和环境变量键值对...");
    for key in ConfigManager::global().keys() {
        if "Path" == key {
            continue;
        }
        info!("{:?}: {:?}", key, ConfigManager::global().get(&key));
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

/// 初始化缓存
///
pub async fn init_cache() {
    let config = ConfigManager::global();
    let _url_value = match config.get("redis.url") {
        Some(url_value) => url_value,
        None => {
            error!("无法从配置管理器获取 redis 配置");
            panic!("无法获取redis配置");
        }
    };

    // let redis_config = RedisConfig::from_config(config);
    let redis_config = config.get_as::<RedisConfig>("redis").unwrap();

    GlobalCacheManager::initialize(redis_config.clone())
        .await
        .expect("redis初始化失败");
    info!("redis缓存初始化完成");
    GlobalLockManager::initialize(redis_config)
        .await
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

    let settings = PluginManagerSettings {
        plugin_root: PathBuf::from(plugin_root),
        backup_root: PathBuf::from(backup_root),
        temp_root: PathBuf::from(temp_root),
        default_database_id: default_db_id,
        node_id: ConfigManager::global().get_string("node.node_id").unwrap_or("default".to_string()),
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
