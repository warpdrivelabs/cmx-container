//! # CMX 工具库
//!
//! 提供常用的工具函数和配置管理功能。
//!
//! ## 功能特性
//!
//! ### 配置管理
//!
//! 基于 `config` crate 实现，支持：
//! - **多配置源支持**: 支持从 TOML/JSON/YAML 文件、环境变量、命令行参数等多种来源加载配置
//! - **优先级机制**: 后添加的配置源优先级更高，自动覆盖先添加的同名配置
//! - **类型转换**: 提供便捷的类型转换 API 和 serde 反序列化
//! - **结构体映射**: 支持将配置反序列化为 Rust 结构体
//! - **全局单例**: `ConfigManager` 提供全局配置管理
//!
//! ## 配置来源优先级（从低到高，按添加顺序）
//!
//! 1. TOML 配置文件
//! 2. .env 文件（通过 dotenvy 加载到环境变量）
//! 3. 系统环境变量
//! 4. 命令行参数（最高优先级）
//!
//! ## 快速开始
//!
//! ### 基本使用
//!
//! ```rust,no_run
//! use cmx_utils::config::{Config, ConfigManager, CommandLineSource};
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // 初始化全局配置
//!     cmx_utils::ConfigManager::initialize(|| {
//!         cmx_utils::Config::builder()
//!             .add_toml_file("config/default.toml", 10)?
//!             .add_env()
//!             .add_command_line(std::env::args().skip(1))
//!             .build()
//!     })?;
//!
//!     // 读取配置值
//!     let host = cmx_utils::ConfigManager::global().get_string("database.host")?;
//!     let port = cmx_utils::ConfigManager::global().get_int("database.port")?;
//!
//!     Ok(())
//! }
//! ```

pub mod b64;
pub mod config;
pub mod crypto;
pub mod id;
pub mod sync_utils;
pub mod time;
pub mod zip;

pub use config::{CommandLineSource, ConfigError, ConfigResult};
pub use config::{
    Config, ConfigBuilder, ConfigManager, DefaultConfigLoader, ConfigStore, ConfigValue,
    FromConfigValue,
};
pub use zip::{ZipCompressor, ZipExtractor, ZipError, ZipResult};
pub use id::{snowflake_id, snowflake_id_str, SnowflakeGenerator, UuidGenerator};
pub use sync_utils::{read_lock, write_lock};
