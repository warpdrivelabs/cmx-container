//! Nacos 错误类型定义
//!
//! 使用 thiserror 定义所有 Nacos 相关的错误类型

use thiserror::Error;

/// Nacos 错误类型
#[derive(Error, Debug)]
pub enum NacosError {
    /// 初始化失败
    #[error("Nacos 初始化失败: {0}")]
    InitFailed(String),

    /// 命名服务未启用
    #[error("命名服务未启用")]
    NamingDisabled,

    /// 配置服务未启用
    #[error("配置服务未启用")]
    ConfigDisabled,

    /// 服务注册失败
    #[error("服务注册失败: {0}")]
    RegisterFailed(String),

    /// 服务注销失败
    #[error("服务注销失败: {0}")]
    DeregisterFailed(String),

    /// 获取配置失败
    #[error("获取配置失败: {0}")]
    ConfigGetFailed(String),

    /// 配置解析失败
    #[error("配置解析失败: {0}")]
    ConfigParseFailed(String),

    /// 配置监听失败
    #[error("配置监听失败: {0}")]
    ConfigListenFailed(String),

    /// 查询失败
    #[error("查询失败: {0}")]
    QueryFailed(String),
}

/// Nacos 操作结果类型别名
pub type NacosResult<T> = core::result::Result<T, NacosError>;
