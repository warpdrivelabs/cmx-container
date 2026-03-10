//! 配置管理模块
//!
//! 提供统一的配置管理功能，支持多种配置来源和格式

// 导出子模块
mod error;
mod value;
mod parser;
mod source;
pub mod config;

// 重新导出常用类型和函数
pub use error::{ConfigError, ConfigResult};
pub use value::{ConfigValue, ConfigStore, FromConfigValue};
pub use parser::{ConfigParser, TomlParser, JsonParser, EnvParser, parse_file_auto};
pub use source::{ConfigSource, Priority, FileSource, EnvSource, CommandLineSource, MemorySource};
pub use config::{Config, ConfigBuilder,DefaultConfigLoader, ConfigManager};
