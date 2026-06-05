//! 注册中心模块
//!
//! 提供服务注册中心的 trait 定义、工厂函数和实现。

mod mock;
mod nacos;
pub mod trait_rs;

pub use mock::MockRegistry;
pub use nacos::NacosRegistry;
pub use trait_rs::{ServiceInstance, ServiceRegistry};

use std::sync::Arc;

use crate::config::RegistryConfig;
use crate::error::RegistryError;

/// 根据配置创建注册中心实例
pub fn create_registry(config: &RegistryConfig) -> Result<Arc<dyn ServiceRegistry>, RegistryError> {
    if !config.enabled {
        tracing::info!("服务注册未启用，使用 MockRegistry");
        return Ok(Arc::new(MockRegistry::new()));
    }

    match config.registry_type.as_str() {
        "nacos" => Ok(Arc::new(NacosRegistry::new(&config.nacos)?)),
        "mock" => Ok(Arc::new(MockRegistry::new())),
        other => Err(RegistryError::UnsupportedType(other.to_string())),
    }
}
