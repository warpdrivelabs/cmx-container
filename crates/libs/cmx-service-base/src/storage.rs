//! 文件存储服务初始化（feature `storage`）。
//!
//! 从 web-server `config/storage.rs` 原样提取：读 ConfigManager `[storage]` → StorageManager →
//! GlobalStorageService::initialize + 本地静态访问路由。**依赖 DB manager 已起**（文件元信息管理），
//! 故须在 datasource 之后调。纯基础设施，零 portal 业务。

use std::sync::Arc;

use cmx_database::get_default_db_manager;
use cmx_storage::config::StorageManagerConfig;
use cmx_storage::global::GlobalStorageService;
use cmx_storage::manager::StorageManager;
use cmx_storage::service::DefaultStorageService;
use cmx_utils::ConfigManager;
use tracing::info;

use crate::{BaseError, Result};

/// 初始化文件存储服务（读全局 ConfigManager 的 `[storage]`）。
pub async fn init_storage() -> Result<()> {
    info!("初始化文件存储服务...");
    let config = ConfigManager::global();
    let storage_config = StorageManagerConfig::from_config(&config)
        .map_err(|e| BaseError::Setup(format!("存储配置加载失败: {e}")))?;
    let manager = Arc::new(
        StorageManager::new(&storage_config)
            .map_err(|e| BaseError::Setup(format!("存储管理器初始化失败: {e}")))?,
    );
    let local_access_configs: Vec<(String, String)> = manager
        .get_local_access_configs()
        .into_iter()
        .map(|(pattern, path)| (pattern.to_string(), path.to_string()))
        .collect();
    let db_manager = get_default_db_manager();
    let service: Arc<dyn cmx_storage::service::StorageService> =
        Arc::new(DefaultStorageService::new(manager, db_manager));
    GlobalStorageService::initialize(service)
        .map_err(|e| BaseError::Setup(format!("存储服务全局初始化失败: {e}")))?;
    if !local_access_configs.is_empty() {
        for (pattern, path) in &local_access_configs {
            info!("注册本地文件静态访问路由: {} -> {}", pattern, path);
        }
        GlobalStorageService::init_local_access_configs(local_access_configs);
    }
    info!("文件存储服务初始化完成");
    Ok(())
}
