//! RPC 调用错误类型。

/// RPC 调用错误类型。
#[derive(Debug, thiserror::Error)]
pub enum RpcError {
    /// 服务未找到。
    #[error("服务未找到: {0}")]
    ServiceNotFound(String),
    /// 无可用实例。
    #[error("无可用实例: {0}")]
    NoAvailableInstance(String),
    /// RPC 调用失败。
    #[error("RPC 调用失败: {0}")]
    RpcCallFailed(String),
    /// 不支持的协议。
    #[error("不支持的协议: {0}")]
    UnsupportedProtocol(String),
    /// 调用超时。
    #[error("调用超时: {0}")]
    Timeout(String),
    /// 认证失败(服务凭证缺失或无效)。
    #[error("认证失败: {0}")]
    Unauthenticated(String),
    /// 权限不足。
    #[error("权限不足: {0}")]
    PermissionDenied(String),
}
