//! 全局存储服务单例模块
//!
//! 提供 `GlobalStorageService` 全局单例，在应用启动时初始化一次，
//! 后续通过 `GlobalStorageService::get()` 获取全局存储服务实例。

use std::sync::{Arc, OnceLock};

use crate::error::Error;
use crate::service::StorageService;

/// 全局存储服务
///
/// 在应用启动时通过 [`GlobalStorageService::initialize`] 初始化一次，
/// 后续通过 [`GlobalStorageService::get`] 获取全局实例。
pub struct GlobalStorageService {
    service: Arc<dyn StorageService>,
}

/// 全局存储服务实例
static GLOBAL_STORAGE_SERVICE: OnceLock<GlobalStorageService> = OnceLock::new();

/// 本地文件静态访问配置
///
/// 每项为 `(path_patterns, storage_path)`：
/// - `path_patterns`：axum 路由路径前缀（如 `/file`）
/// - `storage_path`：本地物理目录路径（如 `/data/cmx/storage`）
static LOCAL_ACCESS_CONFIGS: OnceLock<Vec<(String, String)>> = OnceLock::new();

impl GlobalStorageService {
    /// 初始化全局存储服务
    ///
    /// # Arguments
    ///
    /// * `service` - 存储服务实例
    ///
    /// # Returns
    ///
    /// 初始化成功返回 `Ok(())`，若已初始化则返回错误。
    pub fn initialize(service: Arc<dyn StorageService>) -> Result<(), Error> {
        GLOBAL_STORAGE_SERVICE
            .set(GlobalStorageService { service })
            .map_err(|_| Error::ConfigError("全局存储服务已初始化".to_string()))?;
        Ok(())
    }

    /// 初始化本地文件静态访问配置。
    ///
    /// 在 `init_storage` 中调用，将所有 `enable_access=true` 的本地存储配置
    /// 保存到全局，供 main.rs 构建 axum 静态文件路由使用。
    ///
    /// # Arguments
    ///
    /// * `configs` - 本地存储访问配置列表，每项为 `(path_patterns, storage_path)`
    pub fn init_local_access_configs(configs: Vec<(String, String)>) {
        let _ = LOCAL_ACCESS_CONFIGS.set(configs);
    }

    /// 获取全局存储服务引用
    ///
    /// # Panics
    ///
    /// 若全局存储服务未初始化将 panic。
    ///
    /// # Returns
    ///
    /// 返回全局存储服务静态引用。
    pub fn get() -> &'static GlobalStorageService {
        GLOBAL_STORAGE_SERVICE
            .get()
            .expect("全局存储服务未初始化，请先调用 initialize()")
    }

    /// 获取存储服务实例
    ///
    /// # Returns
    ///
    /// 返回存储服务的 `Arc` 引用。
    pub fn service(&self) -> &Arc<dyn StorageService> {
        &self.service
    }

    /// 获取本地文件静态访问配置。
    ///
    /// # Returns
    ///
    /// 返回所有启用直接访问的本地存储配置列表。
    /// 每项为 `(path_patterns, storage_path)`。
    pub fn local_access_configs() -> &'static [(String, String)] {
        LOCAL_ACCESS_CONFIGS
            .get()
            .map(|c| c.as_slice())
            .unwrap_or(&[])
    }
}
