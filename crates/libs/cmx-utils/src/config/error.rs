//! 配置管理错误处理模块
//!
//! 提供配置管理过程中可能出现的各种错误类型定义
//! 基于 `config::ConfigError` 进行包装，同时保留自定义错误变体

use std::path::PathBuf;
use thiserror::Error;

/// 配置管理错误类型
///
/// 包装 `config::ConfigError` 并保留自定义错误变体，
/// 统一配置管理过程中可能出现的所有错误类型
#[derive(Error, Debug)]
pub enum ConfigError {
    /// 底层 config crate 错误
    #[error("{0}")]
    ConfigError(#[from] config::ConfigError),

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

    /// 配置文件未找到错误
    #[error("配置文件未找到: {path}")]
    FileNotFound {
        /// 文件路径
        path: PathBuf,
    },
}

/// 配置结果类型别名
pub type ConfigResult<T> = Result<T, ConfigError>;
