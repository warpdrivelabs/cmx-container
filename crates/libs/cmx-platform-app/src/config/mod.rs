//! 配置模块
//!
//! 提供应用程序初始化所需的各项配置功能。

pub mod audit;
pub mod auth;
pub mod cache;
pub mod code;
pub mod datasource;
pub mod iam;
pub mod jobs;
pub mod migration;
// storage / services / plugins / infra_init(Nacos) / rpc(init_rpc) 已下沉公用包 cmx-service-base。
// rpc.rs 只留 portal 专属组装（build_function_invoker 绑 cmx-biz / load_outgoing_credential 绑 cmx-plugin）。
pub mod rpc;
pub mod runtime;

pub use datasource::init_datasources;

/// 部署模式（从 cmx-utils re-export，供 web-server 各模块使用）。
pub use cmx_utils::config::DeployMode;

pub use audit::build_audit_logger;
pub use auth::{init_auth_service, init_system_identity};
pub use cache::init_cache;
pub use code::init_code_engine;
pub use iam::{finalize_iam_state, init_iam_services, run_permission_check};
pub use jobs::init_job_center;
pub use migration::init_database_migrations;
pub use rpc::build_function_invoker;
pub use runtime::init_runtime;

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
    WEB_CONFIG_INSTANCE
        .get()
        .ok_or_else(|| ConfigError::BuildError {
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

/// 应用标识配置（当前实例所属的域/应用/模块）。
///
/// 用于数据源过滤：`load_active_datasources` 仅加载归属本实例域的数据源。
/// 从 `[app]` TOML 节读取，支持环境变量覆盖（`APP__DOMAIN_CODE` 等）。
/// 三项均缺省为 `"default"`，保证向后兼容。
#[derive(Debug, Clone)]
pub struct AppIdentity {
    /// 当前实例所属域编码。
    pub domain_code: String,
    /// 当前实例所属应用编码。
    pub application_code: String,
    /// 当前实例所属模块编码。
    pub module_code: String,
}

impl Default for AppIdentity {
    fn default() -> Self {
        Self {
            domain_code: "default".to_string(),
            application_code: "default".to_string(),
            module_code: "default".to_string(),
        }
    }
}

/// 从 `[app]` TOML 节加载应用标识配置。
///
/// 读取 `app.domain_code` / `app.application_code` / `app.module_code`，
/// 未配置时各项回退为 `"default"`（向后兼容）。
pub fn load_app_identity() -> AppIdentity {
    let config = ConfigManager::global();
    let mut identity = AppIdentity::default();
    if let Ok(v) = config.get_string("app.domain_code") {
        identity.domain_code = v;
    }
    if let Ok(v) = config.get_string("app.application_code") {
        identity.application_code = v;
    }
    if let Ok(v) = config.get_string("app.module_code") {
        identity.module_code = v;
    }
    identity
}

/// 加载部署模式（`[deploy] mode`）。
///
/// 未配置时缺省为 `DeployMode::Mono`（向后兼容）。
pub fn load_deploy_mode() -> DeployMode {
    DeployMode::from_config()
}
