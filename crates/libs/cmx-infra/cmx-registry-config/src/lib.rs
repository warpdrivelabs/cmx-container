//! cmx-registry-config: 注册中心与配置中心可扩展抽象层。
//!
//! 该 crate 是 cmx-container 基础设施层的一部分，提供 `ServiceRegistry` 和 `ConfigCenter`
//! 两个核心 trait 的抽象实现，通过工厂函数和 `dyn trait` 动态派发实现配置驱动的实现切换。
//!
//! # 核心功能
//!
//! - **服务注册/发现**：通过 [`ServiceRegistry`](crate::ServiceRegistry) trait 抽象，支持 Nacos、Mock 等实现。
//! - **配置中心**：通过 [`ConfigCenter`](crate::ConfigCenter) trait 抽象，支持远程配置获取和变更监听。
//! - **配置驱动**：通过环境变量或 TOML 配置选择具体实现。
//! - **环境变量兼容**：保持现有 `NACOS_*` 环境变量完全兼容。
//!
//! # 配置优先级（从高到低）
//!
//! 1. 环境变量
//! 2. 远程配置中心
//! 3. 本地 TOML 配置文件
//! 4. 代码默认值
//!
//! # 架构分层
//!
//! ```text
//! ┌────────────────────────────────────────────┐
//! │  应用层 (web-server 等)                      │
//! │  通过 GlobalServiceRegistry / GlobalConfigCenter│
//! │  访问注册/配置中心                            │
//! ├────────────────────────────────────────────┤
//! │  抽象层 (本 crate)                            │
//! │  ServiceRegistry / ConfigCenter trait       │
//! │  + 工厂函数 create_registry/create_config_  │
//! │    center 配置驱动派发                       │
//! ├────────────────────────────────────────────┤
//! │  实现层                                       │
//! │  NacosRegistry / NacosConfigCenter          │
//! │  MockRegistry / MockConfigCenter            │
//! │  (未来扩展) ConsulRegistry / ApolloCenter   │
//! └────────────────────────────────────────────┘
//! ```
//!
//! # 快速开始
//!
//! ```ignore
//! use cmx_registry_config::{
//!     create_registry, create_config_center,
//!     RegistryConfig, ConfigCenterFullConfig,
//! };
//!
//! let registry_cfg = RegistryConfig::from_env();
//! let config_cfg = ConfigCenterFullConfig::from_env();
//!
//! let registry = create_registry(&registry_cfg)?;
//! let config_center = create_config_center(&config_cfg)?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

pub mod config_center;
pub mod config_model;
pub mod config_source;
pub mod error;
pub mod global_config_center;
pub mod global_instance_cache;
pub mod global_registry;
pub mod notifier;
pub mod reloader;
pub mod registry;
pub(crate) mod utils;

#[cfg(test)]
mod tests;

pub use config_model::{
    ConfigCenterFullConfig, ConfigListener, NacosConfigCenterConfig, NacosNamingConfig,
    RegistryConfig,
};
pub use config_center::{create_config_center, ConfigCenter, ConfigChangeCallback, MockConfigCenter, NacosConfigCenter};
pub use config_source::RemoteConfigSource;
pub use error::{ConfigCenterError, RegistryError};
pub use global_config_center::GlobalConfigCenter;
pub use global_registry::GlobalServiceRegistry;
pub use notifier::{ChangeNotifier, ConfigChangeEvent, ConfigChangeListener, GlobalChangeNotifier};
pub use reloader::ConfigReloader;
pub use global_instance_cache::GlobalServiceInstanceCache;
pub use registry::{
    create_registry, create_registry_with_cache, InstanceChangeCallback, MockRegistry,
    NacosRegistry, ServiceInstance, ServiceInstanceCache, ServiceListSyncer, ServiceRegistry,
};
