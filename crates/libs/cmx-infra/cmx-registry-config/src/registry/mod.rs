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

mod mock;
mod nacos;
pub mod trait_rs;

pub use mock::MockRegistry;
pub use nacos::NacosRegistry;
pub use trait_rs::{ServiceInstance, ServiceRegistry};

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
///
/// # Arguments
///
/// * `config` - 注册中心配置，包含启用标志、类型选择和 Nacos 连接参数。
///
/// # Returns
///
/// * `Ok(Arc<dyn ServiceRegistry>)` - 成功返回动态分发的注册中心实例。
/// * `Err(RegistryError::UnsupportedType)` - 配置中指定了未实现的注册中心类型。
/// * `Err(RegistryError::InitFailed)` - Nacos 客户端初始化失败。
///
/// # Examples
///
/// ```ignore
/// use cmx_registry_config::{create_registry, RegistryConfig};
///
/// let config = RegistryConfig::from_env();
/// let registry = create_registry(&config)?;
/// # Ok::<(), cmx_registry_config::RegistryError>(())
/// ```
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
