//! Web 服务器错误模块
//!
//! 定义应用程序级别的错误类型，用于初始化阶段和运行时的错误处理。

use thiserror::Error;

/// 结果类型，包含成功值或错误
pub type Result<T> = core::result::Result<T, Error>;

/// Web 服务器错误类型
///
/// 所有模块级别的错误都应转换为该类型，使用 `?` 操作符传播。
#[derive(Error, Debug)]
pub enum Error {
    /// 配置加载、解析或验证失败。
    #[error("配置错误: {0}")]
    ConfigError(String),

    /// 服务器启动设置、地址绑定或网络配置失败。
    #[error("服务器设置错误: {0}")]
    ServerSetup(String),

    /// 数据源连接、注册或初始化失败。
    #[error("数据源初始化错误: {0}")]
    DatasourceInit(String),

    /// WASM 运行时引擎、宿主函数注册失败。
    #[error("运行时初始化错误: {0}")]
    RuntimeInit(String),

    /// 插件管理器初始化失败。
    #[error("插件初始化错误: {0}")]
    PluginInit(String),

    /// 服务管理器初始化失败。
    #[error("服务初始化错误: {0}")]
    ServiceInit(String),

    /// 存储配置加载或存储服务初始化失败。
    #[error("存储初始化错误: {0}")]
    StorageInit(String),

    /// 数据库迁移执行失败。
    #[error("迁移执行错误: {0}")]
    Migration(String),

    /// IO 操作失败，自动从 std::io::Error 转换。
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
}

