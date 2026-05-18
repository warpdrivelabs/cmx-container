//! 插件管理器模块
//!
//! 提供插件管理器的初始化功能。

use cmx_database::get_default_db_manager;
use cmx_plugin::{GlobalPluginManager, PluginManagerSettings};
use cmx_utils::ConfigManager;
use std::path::PathBuf;
use tracing::info;

pub use crate::Error;

/// 初始化插件管理器。
///
/// 必须在 init_runtime 之后调用，因为需要注册 PluginHostFunctions。
///
/// # Returns
///
/// * `Ok(())` - 插件管理器初始化成功
/// * `Err(Error::PluginInit)` - 插件管理器初始化失败
pub async fn init_plugins() -> crate::Result<()> {
    info!("初始化插件管理器...");

    let default_db_id = get_default_db_manager().get_default_db_id().await;
    let plugin_root = ConfigManager::global()
        .get_string("plugin.install_root")
        .unwrap_or("plugins/root".to_string());
    let backup_root = ConfigManager::global()
        .get_string("plugin.backup_root")
        .unwrap_or("plugins/backup".to_string());
    let temp_root = ConfigManager::global()
        .get_string("plugin.temp_root")
        .unwrap_or("plugins/temp".to_string());

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
        .map_err(|e| Error::PluginInit(format!("初始化插件管理器失败: {}", e)))?;
    info!("成功初始化插件管理器");

    Ok(())
}
