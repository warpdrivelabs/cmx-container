//! chassis 错误类型：启动/配置/绑定阶段的错误。

use thiserror::Error;

/// chassis 结果类型。
pub type Result<T> = core::result::Result<T, ChassisError>;

/// 通用服务骨架错误。
#[derive(Error, Debug)]
pub enum ChassisError {
    /// 配置加载/解析失败（toml 或环境变量）。
    #[error("配置错误: {0}")]
    Config(String),

    /// 服务器绑定/运行失败。
    #[error("服务器设置错误: {0}")]
    ServerSetup(String),

    /// 某个启动钩子返回致命错误（服务无法就绪）。
    #[error("启动钩子[{name}]失败: {source}")]
    InitHook {
        /// 钩子名（便于定位）。
        name: String,
        /// 钩子的原始错误。
        #[source]
        source: anyhow::Error,
    },

    /// IO 错误（绑定端口等）。
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
}
