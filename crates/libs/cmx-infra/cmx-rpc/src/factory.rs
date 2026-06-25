//! RPC 客户端工厂。
//!
//! 迭代 [`crate::bundle::default_bundles`] 初始化各领域客户端，注册到领域全局单例。
//! 工厂本身不关心具体领域，新增领域零改动（OCP）。

use std::sync::Arc;

use cmx_registry_config::registry::{ServiceInstanceCache, ServiceRegistry};
use cmx_traits::rpc::RpcError;

use crate::bundle::{self, RpcServiceBundle};
use crate::client::infra::GrpcInfrastructure;
use crate::config::RpcConfig;
use crate::global::{GlobalRpcClient, GlobalRpcClientAlreadySetError};

/// 客户端初始化错误。
#[derive(thiserror::Error, Debug)]
pub enum ClientInitError {
    /// RPC 错误（如不支持的协议、服务发现失败等）。
    #[error(transparent)]
    Rpc(#[from] RpcError),
    /// 全局 RPC 客户端已初始化（重复初始化）。
    #[error(transparent)]
    AlreadySet(#[from] GlobalRpcClientAlreadySetError),
}

/// 初始化全部内置领域客户端。
///
/// 返回初始化完成的 Bundle 列表，调用方应将其传给
/// [`crate::server_runner::start_grpc_server`] 以注册服务端。
///
/// # Errors
///
/// - [`ClientInitError::Rpc`]：协议不是 `"grpc"`。
/// - [`ClientInitError::AlreadySet`]：重复初始化。
pub fn init_rpc_clients(
    config: &RpcConfig,
    cache: Arc<ServiceInstanceCache>,
    registry: Arc<dyn ServiceRegistry>,
) -> Result<Vec<Box<dyn RpcServiceBundle>>, ClientInitError> {
    if config.protocol != "grpc" {
        return Err(ClientInitError::Rpc(RpcError::UnsupportedProtocol(
            config.protocol.clone(),
        )));
    }

    tracing::info!(
        target: "cmx_rpc",
        protocol = %config.protocol,
        timeout_ms = config.grpc.timeout_ms,
        "初始化 RPC 客户端（gRPC）"
    );

    let infra = Arc::new(GrpcInfrastructure::new(cache, config.grpc.clone(), registry));
    let bundles = bundle::default_bundles();
    for b in &bundles {
        b.init_client(infra.clone());
    }
    GlobalRpcClient::mark_initialized()?;

    Ok(bundles)
}
