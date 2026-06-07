//! 配置模块
//!
//! 提供应用程序初始化所需的各项配置功能。

pub mod cache;
pub mod datasource;
pub mod infra_init;
pub mod migration;
// pub mod nacos;
pub mod plugins;
pub mod rpc;
pub mod runtime;
pub mod services;
pub mod storage;

pub use datasource::init_datasources;

pub use cache::init_cache;
pub use infra_init::{init_infra, shutdown_infra};
pub use migration::init_database_migrations;
// pub use nacos::{init_global_config_with_nacos, shutdown_nacos};
pub use plugins::init_plugins;
pub use rpc::init_rpc;
pub use runtime::init_runtime;
pub use services::init_services;
pub use services::init_service_invoker;
pub use storage::init_storage;

use cmx_utils::{ConfigManager, ConfigResult};
use serde::Deserialize;
use std::sync::OnceLock;

pub fn web_config() -> &'static WebConfig {
    static INSTANCE: OnceLock<WebConfig> = OnceLock::new();

    INSTANCE.get_or_init(|| {
        WebConfig::load_from_env()
            .unwrap_or_else(|ex| panic!("FATAL - WHILE LOADING CONF - Cause: {}", ex))
    })
}

/// Web 服务器配置结构。
///
/// 包含 Web 服务器运行所需的静态配置信息。
#[derive(Debug, Deserialize)]
pub struct WebConfig {
    /// Web 静态文件目录的路径。
    pub web_folder: String,
}

impl WebConfig {
    /// 从环境变量加载配置。
    ///
    /// # Returns
    ///
    /// 成功时返回 WebConfig 实例。
    fn load_from_env() -> ConfigResult<WebConfig> {
        let result = ConfigManager::global().get_string("web_folder");

        match result {
            Ok(value) => Ok(WebConfig { web_folder: value }),
            Err(ex) => Err(ex),
        }
    }
}
