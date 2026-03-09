//! 配置管理错误处理模块
//!
//! 提供配置管理过程中可能出现的各种错误类型定义

use std::path::PathBuf;
use thiserror::Error;

/// 配置管理错误类型
///
/// 定义了配置加载、解析、合并等过程中可能出现的所有错误类型
#[derive(Error, Debug)]
pub enum ConfigError {
    /// 配置文件未找到错误
    #[error("配置文件未找到: {path}")]
    FileNotFound {
        /// 文件路径
        path: PathBuf,
    },

    /// 配置文件读取错误
    #[error("读取配置文件失败: {path}, 原因: {source}")]
    FileReadError {
        /// 文件路径
        path: PathBuf,
        /// 底层IO错误
        source: std::io::Error,
    },

    /// TOML解析错误
    #[error("解析TOML配置失败: {source}")]
    TomlParseError {
        /// TOML解析错误
        source: toml::de::Error,
    },

    /// JSON解析错误
    #[error("解析JSON配置失败: {source}")]
    JsonParseError {
        /// JSON解析错误
        source: serde_json::Error,
    },

    /// .env文件解析错误
    #[error("解析.env文件失败: {message}")]
    EnvParseError {
        /// 错误信息
        message: String,
    },

    /// 配置键不存在错误
    #[error("配置键不存在: {key}")]
    KeyNotFound {
        /// 配置键
        key: String,
    },

    /// 类型转换错误
    #[error("配置值类型转换失败: 无法将键 '{key}' 的值转换为 {target_type}")]
    TypeConversionError {
        /// 配置键
        key: String,
        /// 目标类型
        target_type: String,
    },

    /// 环境变量读取错误
    #[error("读取环境变量失败: {var_name}")]
    EnvVarError {
        /// 环境变量名
        var_name: String,
    },

    /// 配置合并冲突错误
    #[error("配置合并冲突: 键 '{key}' 在多个源中存在冲突值")]
    MergeConflict {
        /// 冲突的配置键
        key: String,
    },

    /// 无效的配置路径错误
    #[error("无效的配置路径: {path}")]
    InvalidPath {
        /// 无效路径
        path: String,
    },

    /// 配置构建错误
    #[error("配置构建失败: {message}")]
    BuildError {
        /// 错误信息
        message: String,
    },
    
    /// 无效的优先级错误
    #[error("无效的优先级: {priority}, 优先级必须在 0-100 之间")]
    InvalidPriority {
        /// 无效的优先级值
        priority: u8,
    },
}

/// 配置结果类型别名
pub type ConfigResult<T> = Result<T, ConfigError>;
