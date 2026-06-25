//! 错误类型定义。
//!
//! 该模块集中定义注册中心与配置中心相关操作的错误类型。
//! 遵循项目 `thiserror` 规范，所有错误使用 `#[derive(thiserror::Error)]` 派生。

use thiserror::Error;

/// 注册中心错误。
///
/// 涵盖注册中心生命周期内可能出现的所有错误场景。
#[derive(Error, Debug)]
pub enum RegistryError {
    /// 注册中心未启用，调用方不应执行注册/注销操作。
    #[error("注册中心未启用")]
    Disabled,

    /// 配置中指定了未实现的注册中心类型。
    #[error("不支持的注册中心类型: {0}")]
    UnsupportedType(String),

    /// 服务实例注册失败。
    #[error("服务注册失败: {0}")]
    RegisterFailed(String),

    /// 服务实例注销失败。
    #[error("服务注销失败: {0}")]
    DeregisterFailed(String),

    /// 服务实例查询失败。
    #[error("实例查询失败: {0}")]
    QueryFailed(String),

    /// 客户端初始化失败。
    #[error("初始化失败: {0}")]
    InitFailed(String),
}

/// 配置中心错误。
///
/// 涵盖配置中心生命周期内可能出现的所有错误场景。
#[derive(Error, Debug)]
pub enum ConfigCenterError {
    /// 配置中心未启用，调用方不应执行获取/监听操作。
    #[error("配置中心未启用")]
    Disabled,

    /// 配置中指定了未实现的配置中心类型。
    #[error("不支持的配置中心类型: {0}")]
    UnsupportedType(String),

    /// 远程配置获取失败（网络错误、配置不存在等）。
    #[error("配置获取失败: {0}")]
    GetFailed(String),

    /// 配置内容解析失败（TOML 格式错误等）。
    #[error("配置解析失败: {0}")]
    ParseFailed(String),

    /// 注册配置变更监听失败。
    #[error("配置监听失败: {0}")]
    ListenFailed(String),

    /// 客户端初始化失败。
    #[error("初始化失败: {0}")]
    InitFailed(String),

    /// 配置热更新失败（全局配置替换失败）。
    #[error("配置热更新失败: {0}")]
    ReloadFailed(String),
}

/// 全局存储器（注册中心 / 配置中心 / 实例缓存）重复初始化错误。
///
/// 三个全局存储器（`GlobalServiceRegistry`、`GlobalConfigCenter`、`GlobalServiceInstanceCache`）
/// 共用此错误类型，消除重复定义。具体哪个存储器触发由调用上下文确定。
#[derive(Error, Debug, Clone, Copy, PartialEq, Eq)]
#[error("全局存储已初始化，无法重复设置")]
pub struct GlobalStorageError;

impl GlobalStorageError {
    /// 重复设置全局存储器时返回的错误实例。
    pub const ALREADY_SET: Self = Self;
}
