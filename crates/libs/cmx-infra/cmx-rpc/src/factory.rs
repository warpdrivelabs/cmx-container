//! RPC 客户端工厂
//!
//! 根据协议类型创建对应的 RpcClient 实现。

use std::sync::Arc;

use cmx_registry_config::registry::{ServiceInstanceCache, ServiceRegistry};
use cmx_traits::rpc::{RpcClient, RpcError};

use crate::client::VoloGrpcClient;
use crate::config::RpcConfig;

/// 根据协议创建 RPC 客户端
///
/// 目前仅支持 "grpc" 协议，后续可扩展其他协议。
pub fn create_rpc_client(
    config: &RpcConfig,
    cache: Arc<ServiceInstanceCache>,
    registry: Arc<dyn ServiceRegistry>,
) -> Result<Arc<dyn RpcClient>, RpcError> {
    match config.protocol.as_str() {
        "grpc" => {
            tracing::info!(
                target: "cmx_rpc",
                protocol = %config.protocol,
                timeout_ms = config.grpc.timeout_ms,
                "创建 gRPC RPC 客户端"
            );
            Ok(Arc::new(VoloGrpcClient::new(cache, config.grpc.clone(), registry)))
        }
        "http_rest" => {
            Err(RpcError::UnsupportedProtocol(
                "http_rest 协议暂未实现".to_string(),
            ))
        }
        other => Err(RpcError::UnsupportedProtocol(other.to_string())),
    }
}
