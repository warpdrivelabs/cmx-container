//! 配置模块
//!
//! 提供应用程序初始化所需的各项配置功能。

pub mod auth;
pub mod audit;
pub mod cache;
pub mod datasource;
pub mod iam;
pub mod infra_init;
pub mod migration;
// pub mod nacos;
pub mod plugins;
pub mod rpc;
pub mod runtime;
pub mod services;
pub mod storage;

pub use datasource::init_datasources;

pub use auth::init_auth_service;
pub use audit::build_audit_logger;
pub use cache::init_cache;
pub use iam::{init_iam_services, finalize_iam_state};
pub use infra_init::{init_infra, shutdown_infra};
pub use migration::init_database_migrations;
// pub use nacos::{init_global_config_with_nacos, shutdown_nacos};
pub use plugins::init_plugins;
pub use runtime::init_runtime;
pub use rpc::init_rpc;
pub use services::init_services;
pub use services::init_service_invoker;
pub use storage::init_storage;

use cmx_utils::{ConfigError, ConfigManager, ConfigResult};
use serde::Deserialize;
use std::sync::OnceLock;

/// Web 服务器配置单例。
///
/// 由 [`init_web_config`] 在启动早期完成初始化，运行期通过 [`web_config`] 读取。
static WEB_CONFIG_INSTANCE: OnceLock<WebConfig> = OnceLock::new();

/// 初始化 Web 服务器配置。
///
/// 从环境变量加载配置并存储到全局单例。必须在 [`web_config`] 调用前完成。
///
/// # Errors
///
/// 配置加载失败时返回 `ConfigError`。
pub fn init_web_config() -> ConfigResult<()> {
    let config = WebConfig::load_from_env()?;
    // 重复初始化时保留首次值，忽略后续 set 返回的错误。
    let _ = WEB_CONFIG_INSTANCE.set(config);
    Ok(())
}

/// 获取 Web 服务器配置的静态引用。
///
/// 必须先调用 [`init_web_config`] 完成初始化。
///
/// # Errors
///
/// 配置未初始化时返回 `ConfigError`。
pub fn web_config() -> ConfigResult<&'static WebConfig> {
    WEB_CONFIG_INSTANCE.get().ok_or_else(|| ConfigError::BuildError {
        message: "WebConfig 尚未初始化，请先调用 init_web_config()".to_string(),
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
