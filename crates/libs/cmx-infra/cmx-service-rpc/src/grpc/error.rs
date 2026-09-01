//! RPC 框架错误类型定义

use thiserror::Error;

/// RPC 框架层错误
///
/// 用于 RPC 框架内部错误场景，与 cmx-traits 的 RpcError 区分。
#[derive(Error, Debug)]
pub enum RpcFrameworkError {
    /// gRPC 服务启动失败
    #[error("gRPC 服务启动失败: {0}")]
    ServerStartFailed(String),

    /// 注册中心未初始化
    #[error("注册中心未初始化")]
    RegistryNotInitialized,

    /// 服务发现失败
    #[error("服务发现失败: {0}")]
    DiscoveryFailed(String),
}
