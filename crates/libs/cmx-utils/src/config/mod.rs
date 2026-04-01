//! 配置管理模块
//!
//! 基于 `config` crate 实现的配置管理系统，支持：
//! - TOML/JSON/YAML 配置文件加载
//! - 环境变量读取（支持前缀过滤）
//! - 命令行参数解析
//! - 全局配置管理器（单例模式）
//! - serde 反序列化为强类型结构体
//!
//! # 快速开始
//!
//! ```ignore
//! use cmx_utils::config::{Config, ConfigManager};
//!
//! // 初始化全局配置
//! ConfigManager::initialize(|| {
//!     Config::builder()
//!         .add_toml_file("config/default.toml", 10)?
//!         .add_env()
//!         .build()
//! })?;
//!
//! // 读取配置
//! let host = ConfigManager::global().get_string("database.host")?;
//! ```

pub mod error;
pub mod source;
pub mod value;

mod config;

pub use config::{Config, ConfigBuilder, ConfigManager, DefaultConfigLoader};
pub use error::{ConfigError, ConfigResult};
pub use source::CommandLineSource;
pub use value::{ConfigStore, ConfigValue, FromConfigValue};
