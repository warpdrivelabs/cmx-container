//! 配置中心模块
//!
//! 提供配置中心的 trait 定义、工厂函数和实现。

mod mock;
mod nacos;
pub mod trait_rs;

pub use mock::MockConfigCenter;
pub use nacos::NacosConfigCenter;
pub use trait_rs::{ConfigCenter, ConfigChangeCallback};

use std::sync::Arc;

use crate::config::ConfigCenterFullConfig;
use crate::error::ConfigCenterError;

/// 根据配置创建配置中心实例
pub fn create_config_center(
    config: &ConfigCenterFullConfig,
) -> Result<Arc<dyn ConfigCenter>, ConfigCenterError> {
    if !config.enabled {
        tracing::info!("配置中心未启用，使用 MockConfigCenter");
        return Ok(Arc::new(MockConfigCenter::new()));
    }

    match config.center_type.as_str() {
        "nacos" => Ok(Arc::new(NacosConfigCenter::new(&config.nacos)?)),
        "mock" => Ok(Arc::new(MockConfigCenter::new())),
        other => Err(ConfigCenterError::UnsupportedType(other.to_string())),
    }
}
