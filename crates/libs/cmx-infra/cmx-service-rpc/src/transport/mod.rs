//! 传输层（transport）——把一次 [`RpcRequest`] 落到具体的协议实现。
//!
//! 当前提供 HTTP 实现（默认 feature `http`）；gRPC 走契约 SDK 的专属绑定
//! （`grpc-client` feature），不经本 trait。trait 化是为了契约 SDK 的单测
//! （mock 传输注入，全链路验证目录 / 解包 / 错误映射，不起端口）。

#[cfg(feature = "http")]
mod http;
#[cfg(feature = "http")]
pub use http::HttpTransport;

use std::time::Duration;

use async_trait::async_trait;

use crate::error::ServiceRpcError;
use crate::invoke::{OutgoingHeaders, RpcRequest, RpcResponse};

/// 无 `http` feature 时的占位传输：所有调用直接失败并给出提示（纯 gRPC 进程场景）。
#[derive(Debug, Clone, Default)]
pub struct NoopTransport;

#[async_trait]
impl Transport for NoopTransport {
    async fn execute(
        &self,
        _base: &str,
        req: &RpcRequest,
        _timeout: Duration,
        _headers: &OutgoingHeaders,
    ) -> Result<RpcResponse, ServiceRpcError> {
        Err(ServiceRpcError::Unavailable {
            key: req.key.clone(),
            cause: "进程未启用 http feature，无法执行 HTTP 服务间调用".to_string(),
        })
    }
}

/// 单次调用传输执行接口。
#[async_trait]
pub trait Transport: Send + Sync {
    /// 向 `base`（已解析的目标基址，如 `http://127.0.0.1:8091`）执行请求。
    ///
    /// 职责：URL 拼接（base + path + query）、body 编码、鉴权头落地、总超时；
    /// 返回状态码与已解析 JSON body。**不做**：定位 / 重试 / 熔断 / 业务信封判定
    /// （这些归 [`crate::ServiceRpcHandle::execute`]）。
    async fn execute(
        &self,
        base: &str,
        req: &RpcRequest,
        timeout: Duration,
        headers: &OutgoingHeaders,
    ) -> Result<RpcResponse, ServiceRpcError>;
}
