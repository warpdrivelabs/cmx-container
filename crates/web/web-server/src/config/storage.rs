//! 文件存储模块
//!
//! 提供存储管理器和存储服务的初始化功能。

use cmx_database::get_default_db_manager;
use cmx_storage::config::StorageManagerConfig;
use cmx_storage::global::GlobalStorageService;
use cmx_storage::manager::StorageManager;
use cmx_storage::service::DefaultStorageService;
use cmx_utils::ConfigManager;
use std::sync::Arc;
use tracing::info;

pub use crate::Error;

/// 初始化文件存储服务。
///
/// 从全局配置中加载存储配置，创建 `StorageManager` 和 `DefaultStorageService`，
/// 并注册到 `GlobalStorageService` 全局单例。
///
/// 必须在 `init_datasources` 之后调用，因为存储服务依赖数据库进行文件元信息管理。
///
/// # Returns
///
/// * `Ok(())` - 存储服务初始化成功
/// * `Err(Error::StorageInit)` - 存储配置加载或服务创建失败
pub async fn init_storage() -> crate::Result<()> {
    info!("初始化文件存储服务...");

    let config = ConfigManager::global();
    let storage_config = StorageManagerConfig::from_config(config)
        .map_err(|e| Error::StorageInit(format!("存储配置加载失败: {}", e)))?;

    let manager = Arc::new(
        StorageManager::new(&storage_config)
            .map_err(|e| Error::StorageInit(format!("存储管理器初始化失败: {}", e)))?
    );

    let db_manager = get_default_db_manager();
    let service: Arc<dyn cmx_storage::service::StorageService> =
        Arc::new(DefaultStorageService::new(manager, db_manager));

    GlobalStorageService::initialize(service)
        .map_err(|e| Error::StorageInit(format!("存储服务全局初始化失败: {}", e)))?;

    info!("文件存储服务初始化完成");

    Ok(())
}
