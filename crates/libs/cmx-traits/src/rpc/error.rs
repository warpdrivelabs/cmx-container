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
}
