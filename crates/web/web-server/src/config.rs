/*
 * @Author: yqs
 * @Date: 2026-03-10 15:35:34
 * @Describe:
 * @LastEditors: yqs
 * @LastEditTime: 2026-03-17 19:59:28
 */
//! Web 服务器配置模块

use cmx_buffer::{GlobalCacheManager, GlobalLockManager, RedisConfig};
use cmx_database::{get_default_db_manager, DbConfig};
use cmx_utils::{CommandLineSource, ConfigBuilder, ConfigManager, ConfigResult};
use serde::Deserialize;
use std::sync::OnceLock;
use tracing::{error, info};

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

/// 初始化数据库数据源
///
/// 从配置管理器读取 databases 配置数组，解析为 Vec<DbConfig> 并注册到数据库管理器
pub async fn init_db_datasource() {
    let config = ConfigManager::global();

    let configs: Vec<DbConfig> = match config.get_as("databases") {
        Ok(configs) => configs,
        Err(e) => {
            error!("无法从配置管理器获取 databases 配置: {:?}", e);
            panic!("无法获取数据库配置: {:?}", e);
        }
    };

    info!("成功解析到 {} 个数据源配置", configs.len());

    let db_manager = get_default_db_manager();

    for db_config in &configs {
        match db_manager.register_data_source(db_config.clone()).await {
            Ok(_) => {
                info!(
                    "成功注册数据源: {} (类型: {:?})",
                    db_config.db_id, db_config.db_type
                );
            }
            Err(e) => {
                error!("注册数据源 {} 失败: {}", db_config.db_id, e);
                panic!("注册数据源失败: {}", e);
            }
        }
    }

    info!("数据库数据源初始化完成");
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

    let redis_config = RedisConfig::from_config(config);
    GlobalCacheManager::initialize(redis_config.clone())
        .await
        .expect("redis初始化失败");
    info!("redis缓存初始化完成");
    GlobalLockManager::initialize(redis_config)
        .await
        .expect("redis分布式锁初始化失败");
    info!("redis分布式锁初始化完成");
}
// 在 config.rs 中添加初始化函数
pub async fn init_plugins() {
    use cmx_plugin::{GlobalPluginManager, PluginManagerSettings};
    use std::path::PathBuf;

    // 方式1：使用默认配置初始化
    // GlobalPluginManager::initialize(Default::default())
    //     .await
    //     .expect("插件管理器初始化失败");

    // 方式2：使用自定义配置初始化  todo 自定义配置需要完善

    let default_db_id = get_default_db_manager().get_default_db_id().await;
    let settings = PluginManagerSettings {
        plugin_root: PathBuf::new().join("plugins").join("root"),
        backup_root: PathBuf::new().join("plugins").join("backup"),
        temp_root: PathBuf::new().join("plugins").join("temp"),
        default_database_id: default_db_id,
        node_id: ConfigManager::global().get_string("node.node_id").unwrap_or("default".to_string()),
        ..Default::default()
    };
    GlobalPluginManager::initialize(settings)
        .await
        .unwrap_or_else(|e| panic!("初始化插件管理器失败: {:?}", e));
    info!("成功初始化插件管理器");

    // ///方式3：注入外部依赖（推荐）
    // GlobalPluginManager::initialize_with_deps(
    //     Default::default(),
    //     Some(get_default_db_manager()),           // 使用已有的数据库管理器
    //     Some(GlobalCacheManager::get_arc()),      // 使用已有的缓存管理器
    //     None,  // 分布式锁管理器
    //     None,  // 消息订阅发布
    // ).await.expect("插件管理器初始化失败");

    // 安装插件
    let install_req = cmx_plugin::service::install::InstallRequest {
        source: cmx_plugin::domain::plugin::PluginSource::Local {
            path: PathBuf::from("E:/rustspace/cmx/cmx-container/plugin.zip"),
        },
        db_id: None,
        auto_activate: false,
        version_constraint: None,
    };

    let resp = GlobalPluginManager::get().await.install(install_req).await;
    match resp {
        Ok(resp) => {
            info!("插件安装响应: {:?}", resp);
        }
        Err(e) => {
            error!("插件安装失败: {:?}", e);
        }
    }
}
