//! 注册中心模块。
//!
//! 提供服务注册中心的 trait 定义、工厂函数和实现。
//!
//! 工厂函数 [`create_registry`] 根据 [`RegistryConfig`] 中的 `enabled` 和 `registry_type`
//! 字段返回对应实现的 `Arc<dyn ServiceRegistry>`。
//!
//! # 已支持实现
//!
//! - [`MockRegistry`]：内存级实现，适用于本地开发与单元测试。
//! - [`NacosRegistry`]：基于 `nacos-sdk` 的生产级实现。
//!
//! # 扩展新实现
//!
//! 1. 在 `registry/` 子模块下新增实现文件并实现 [`ServiceRegistry`] trait。
//! 2. 在 [`create_registry`] 函数中增加 `match` 分支。
//! 3. 在 [`RegistryConfig`] 中新增对应配置结构。

pub mod instance_cache;
mod mock;
mod nacos;
pub mod service_list_syncer;
pub mod trait_rs;

pub use instance_cache::ServiceInstanceCache;
pub use mock::MockRegistry;
pub use nacos::NacosRegistry;
pub use service_list_syncer::ServiceListSyncer;
pub use trait_rs::{InstanceChangeCallback, ServiceInstance, ServiceRegistry};

use std::sync::Arc;

use crate::config::RegistryConfig;
use crate::error::RegistryError;

/// 根据配置创建注册中心实例。
///
/// 该工厂函数实现配置驱动的实现选择：
/// - `config.enabled == false` —— 强制使用 `MockRegistry`（即使配置了 `nacos`）。
/// - `config.registry_type == "nacos"` —— 创建 `NacosRegistry`。
/// - `config.registry_type == "mock"` —— 创建 `MockRegistry`。
/// - 其他类型 —— 返回 [`RegistryError::UnsupportedType`]。
pub async fn create_registry(config: &RegistryConfig) -> Result<Arc<dyn ServiceRegistry>, RegistryError> {
    let (registry, _cache) = create_registry_with_cache(config).await?;
    Ok(registry)
}

/// 根据配置创建带缓存的注册中心实例。
///
/// 与 [`create_registry`] 类似，但内部创建共享缓存并一并返回。
/// 返回的缓存可配合 [`GlobalServiceInstanceCache`](crate::GlobalServiceInstanceCache) 使用。
pub async fn create_registry_with_cache(
    config: &RegistryConfig,
) -> Result<(Arc<dyn ServiceRegistry>, Arc<ServiceInstanceCache>), RegistryError> {
    let cache = Arc::new(ServiceInstanceCache::new());

    if !config.enabled {
        tracing::info!("服务注册未启用，使用 MockRegistry（带缓存）");
        return Ok((Arc::new(MockRegistry::new_with_cache(cache.clone())), cache));
    }

    match config.registry_type.as_str() {
        "nacos" => {
            let registry = NacosRegistry::new_with_cache(&config.nacos, cache.clone()).await?;
            Ok((Arc::new(registry), cache))
        }
        "mock" => Ok((Arc::new(MockRegistry::new_with_cache(cache.clone())), cache)),
        other => Err(RegistryError::UnsupportedType(other.to_string())),
    }
}
