//! 插件管理器初始化（feature `plugins`）。
//!
//! 从 web-server `config/plugins.rs` 原样提取：读 ConfigManager `plugin.*` → PluginManagerSettings →
//! GlobalPluginManager::initialize。**须在 wasm runtime 之后调**（PluginHostFunctions 先注册）。纯基础设施。

use std::path::PathBuf;

use cmx_database::get_default_db_manager;
use cmx_plugin::{GlobalPluginManager, PluginManagerSettings};
use cmx_utils::ConfigManager;
use tracing::info;

use crate::{BaseError, Result};

/// 初始化插件管理器（读全局 ConfigManager 的 `plugin.*`）。
pub async fn init_plugins() -> Result<()> {
    info!("初始化插件管理器...");

    let default_db_id = get_default_db_manager().get_default_db_id().await;
    let plugin_root = ConfigManager::global()
        .get_string("plugin.install_root")
        .unwrap_or_else(|_| "plugins/root".to_string());
    let backup_root = ConfigManager::global()
        .get_string("plugin.backup_root")
        .unwrap_or_else(|_| "plugins/backup".to_string());
    let temp_root = ConfigManager::global()
        .get_string("plugin.temp_root")
        .unwrap_or_else(|_| "plugins/temp".to_string());

    let auto_install_config = ConfigManager::global()
        .get_as::<cmx_plugin::AutoInstallConfig>("plugin.auto_install")
        .unwrap_or_default();

    let app_id = ConfigManager::global().get_app_id();

    let reconciliation_interval_secs = ConfigManager::global()
        .get_as::<u64>("plugin.reconciliation_interval_secs")
        .unwrap_or(60);

    let settings = PluginManagerSettings {
        plugin_root: PathBuf::from(plugin_root),
        backup_root: PathBuf::from(backup_root),
        temp_root: PathBuf::from(temp_root),
        default_database_id: default_db_id,
        node_id: None,
        app_id,
        reconciliation_interval_secs,
        auto_install: auto_install_config,
        ..Default::default()
    };

    GlobalPluginManager::initialize(settings)
        .await
        .map_err(|e| BaseError::Setup(format!("初始化插件管理器失败: {e}")))?;
    info!("成功初始化插件管理器");

    Ok(())
}
