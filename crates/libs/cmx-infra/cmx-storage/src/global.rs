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
}
