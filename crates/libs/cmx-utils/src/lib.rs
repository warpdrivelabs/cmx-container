//! # CMX 工具库
//!
//! 提供常用的工具函数和配置管理功能。
//!
//! ## 功能特性
//!
//! ### 配置管理
//!
//! - **多配置源支持**: 支持从文件、环境变量、命令行参数等多种来源加载配置
//! - **多格式支持**: 支持 TOML、JSON、.env 三种配置文件格式
//! - **优先级机制**: 高优先级配置自动覆盖低优先级配置
//! - **类型转换**: 提供便捷的类型转换 API
//! - **结构体映射**: 支持将配置反序列化为 Rust 结构体
//!
//! ## 配置来源优先级（从高到低）
//!
//! 1. 命令行参数
//! 2. 系统环境变量
//! 3. .env 文件
//! 4. 用户指定的TOML配置文件（优先级由用户指定）
//!
//! ## 快速开始
//!
//! ### 基本使用
//!
//! ```rust,no_run
//! use cmx_utils::config::{Config, ConfigBuilder, FileSource, EnvSource, CommandLineSource, Priority};
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // 创建配置管理器
//!     let mut builder = Config::builder();
//!
//!     // 添加默认配置文件（优先级 10）
//!     builder = builder.add_toml_file("config/default.toml", 10)?;
//!
//!     // 添加生产环境配置文件（优先级 20）
//!     builder = builder.add_toml_file("config/production.toml", 20)?;
//!
//!     // 添加 .env 文件
//!     builder = builder.add_source(FileSource::env_file(".env"));
//!
//!     // 添加系统环境变量
//!     builder = builder.add_source(EnvSource::new());
//!
//!     // 添加命令行参数
//!     builder = builder.add_source(CommandLineSource::from_args(std::env::args().skip(1)));
//!
//!     // 构建配置
//!     let config = builder.build()?;
//!
//!     // 读取配置值
//!     let host: String = config.get_string("database.host")?;
//!     let port: i64 = config.get_int("database.port")?;
//!
//!     Ok(())
//! }
//! ```

pub mod b64;
pub mod config;
pub mod time;

pub use config::{CommandLineSource, ConfigSource, EnvSource, FileSource, MemorySource, Priority};
pub use config::{ConfigParser, EnvParser, JsonParser, TomlParser, parse_file_auto};
// 重新导出配置模块的常用类型
pub use config::{
    Config, ConfigBuilder, ConfigError, ConfigResult, ConfigStore, ConfigValue, FromConfigValue,
};
pub use config::{ConfigManager, DefaultConfigLoader};
