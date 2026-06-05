//! 配置中心模块。
//!
//! 提供配置中心的 trait 定义、工厂函数和实现。
//!
//! 工厂函数 [`create_config_center`] 根据 [`ConfigCenterFullConfig`] 中的 `enabled` 和
//! `center_type` 字段返回对应实现的 `Arc<dyn ConfigCenter>`。
//!
//! # 已支持实现
//!
//! - [`MockConfigCenter`]：内存级实现，提供 `set_config` / `simulate_change` 测试辅助方法。
//! - [`NacosConfigCenter`]：基于 `nacos-sdk` 的生产级实现，支持配置获取与变更监听。
//!
//! # 扩展新实现
//!
//! 1. 在 `config_center/` 子模块下新增实现文件并实现 [`ConfigCenter`] trait。
//! 2. 在 [`create_config_center`] 函数中增加 `match` 分支。
//! 3. 在 [`ConfigCenterFullConfig`] 中新增对应配置结构。
//!
//! # 变更事件分发
//!
//! 配置中心收到变更后，通过 [`ConfigChangeCallback`] 回调推送到应用，
//! 推荐与 [`GlobalChangeNotifier`](crate::GlobalChangeNotifier) 配合使用，
//! 实现结构化事件分发与配置热更新。

mod mock;
mod nacos;
pub mod trait_rs;

pub use mock::MockConfigCenter;
pub use nacos::NacosConfigCenter;
pub use trait_rs::{ConfigCenter, ConfigChangeCallback};

use std::sync::Arc;

use crate::config::ConfigCenterFullConfig;
use crate::error::ConfigCenterError;

/// 根据配置创建配置中心实例。
///
/// 该工厂函数实现配置驱动的实现选择：
/// - `config.enabled == false` —— 强制使用 `MockConfigCenter`（即使配置了 `nacos`）。
/// - `config.center_type == "nacos"` —— 创建 `NacosConfigCenter`。
/// - `config.center_type == "mock"` —— 创建 `MockConfigCenter`。
/// - 其他类型 —— 返回 [`ConfigCenterError::UnsupportedType`]。
///
/// # Arguments
///
/// * `config` - 配置中心配置，包含启用标志、类型选择和 Nacos 连接参数。
///
/// # Returns
///
/// * `Ok(Arc<dyn ConfigCenter>)` - 成功返回动态分发的配置中心实例。
/// * `Err(ConfigCenterError::UnsupportedType)` - 配置中指定了未实现的配置中心类型。
/// * `Err(ConfigCenterError::InitFailed)` - Nacos 客户端初始化失败。
///
/// # Examples
///
/// ```ignore
/// use cmx_registry_config::{create_config_center, ConfigCenterFullConfig};
///
/// let config = ConfigCenterFullConfig::from_env();
/// let center = create_config_center(&config).await?;
/// # Ok::<(), cmx_registry_config::ConfigCenterError>(())
/// ```
pub async fn create_config_center(
    config: &ConfigCenterFullConfig,
) -> Result<Arc<dyn ConfigCenter>, ConfigCenterError> {
    if !config.enabled {
        tracing::info!("配置中心未启用，使用 MockConfigCenter");
        return Ok(Arc::new(MockConfigCenter::new()));
    }

    match config.center_type.as_str() {
        "nacos" => Ok(Arc::new(NacosConfigCenter::new(&config.nacos).await?)),
        "mock" => Ok(Arc::new(MockConfigCenter::new())),
        other => Err(ConfigCenterError::UnsupportedType(other.to_string())),
    }
}
