//! 错误类型定义

use thiserror::Error;

/// 注册中心错误
#[derive(Error, Debug)]
pub enum RegistryError {
    #[error("注册中心未启用")]
    Disabled,
    #[error("不支持的注册中心类型: {0}")]
    UnsupportedType(String),
    #[error("服务注册失败: {0}")]
    RegisterFailed(String),
    #[error("服务注销失败: {0}")]
    DeregisterFailed(String),
    #[error("实例查询失败: {0}")]
    QueryFailed(String),
    #[error("初始化失败: {0}")]
    InitFailed(String),
}

/// 配置中心错误
#[derive(Error, Debug)]
pub enum ConfigCenterError {
    #[error("配置中心未启用")]
    Disabled,
    #[error("不支持的配置中心类型: {0}")]
    UnsupportedType(String),
    #[error("配置获取失败: {0}")]
    GetFailed(String),
    #[error("配置解析失败: {0}")]
    ParseFailed(String),
    #[error("配置监听失败: {0}")]
    ListenFailed(String),
    #[error("初始化失败: {0}")]
    InitFailed(String),
}
