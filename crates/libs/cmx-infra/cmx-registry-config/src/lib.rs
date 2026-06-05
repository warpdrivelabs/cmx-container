//! cmx-registry-config: 注册中心与配置中心可扩展抽象层
//!
//! 提供 `ServiceRegistry` 和 `ConfigCenter` 两个核心 trait，
//! 通过工厂函数和 `dyn trait` 动态派发实现配置驱动的实现切换。
//!
//! # 核心功能
//!
//! - **服务注册/发现**: 通过 `ServiceRegistry` trait 抽象，支持 Nacos、Mock 等实现
//! - **配置中心**: 通过 `ConfigCenter` trait 抽象，支持远程配置获取和变更监听
//! - **配置驱动**: 通过环境变量或 TOML 配置选择具体实现
//! - **环境变量兼容**: 保持现有 `NACOS_*` 环境变量完全兼容
//!
//! # 配置优先级（从高到低）
//!
//! 1. 环境变量
//! 2. 远程配置中心
//! 3. 本地 TOML 配置文件
//! 4. 代码默认值

pub mod config;
pub mod config_center;
pub mod config_source;
pub mod error;
pub mod global_config_center;
pub mod global_registry;
pub mod notifier;
pub mod registry;

pub use config::{
    ConfigCenterFullConfig, ConfigListener, NacosConfigCenterConfig, NacosNamingConfig,
    RegistryConfig,
};
pub use config_center::{create_config_center, ConfigCenter, ConfigChangeCallback, MockConfigCenter, NacosConfigCenter};
pub use config_source::RemoteConfigSource;
pub use error::{ConfigCenterError, RegistryError};
pub use global_config_center::GlobalConfigCenter;
pub use global_registry::GlobalRegistry;
pub use notifier::{ChangeNotifier, GlobalChangeNotifier};
pub use registry::{create_registry, MockRegistry, NacosRegistry, ServiceInstance, ServiceRegistry};
