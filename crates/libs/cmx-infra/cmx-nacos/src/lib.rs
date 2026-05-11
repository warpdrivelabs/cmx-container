//! cmx-nacos: Nacos 微服务集成库
//!
//! 基于 nacos-sdk-rust 封装，提供服务注册/发现和配置中心功能，
//! 与 cmx-utils 的 Config 框架深度集成，支持远程配置覆盖本地配置。
//!
//! # 核心功能
//!
//! - **服务注册/发现**: 通过 Nacos 命名服务实现微服务自动注册与健康检查
//! - **配置中心**: 通过 Nacos 配置中心实现远程配置管理与热更新
//! - **配置覆盖**: NacosConfigSource 实现 config::Source trait，
//!   远程配置可通过 ConfigBuilder::add_source() 注入，自动覆盖本地同名配置项
//!
//! # 配置优先级（从高到低）
//!
//! 1. 环境变量
//! 2. Nacos 远程配置
//! 3. 本地 TOML 配置文件
//! 4. 代码默认值

pub mod client;
pub mod config;
pub mod config_service;
pub mod config_source;
pub mod error;
pub mod listener;
pub mod naming;
pub mod notifier;

pub use client::NacosClient;
pub use config::{ConfigCenterConfig, ConfigListener, NacosConfig, NamingConfig};
pub use config_source::NacosConfigSource;
pub use error::{NacosError, NacosResult};
pub use listener::RemoteConfigChangeListener;
pub use notifier::{ConfigChangeCallback, ConfigChangeNotifier, GlobalConfigChangeNotifier};
