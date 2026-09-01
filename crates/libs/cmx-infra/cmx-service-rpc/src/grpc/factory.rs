//! RPC 客户端工厂。
//!
//! 迭代**调用方传入**的领域 Bundle 列表初始化各领域客户端，注册到领域全局单例。
//! 工厂本身不关心具体领域，新增领域零改动（OCP）——"主应用提供哪些 RPC 服务"
//! 由组装层（cmx-platform-app）显式收集的皮肤 crate Bundle 列表决定。

use std::sync::Arc;

use cmx_registry_config::registry::{ServiceInstanceCache, ServiceRegistry};
use cmx_traits::rpc::RpcError;

use crate::grpc::bundle::RpcServiceBundle;
use crate::grpc::client::infra::GrpcInfrastructure;
use crate::config::ServerConfig;
use crate::grpc::global::{GlobalRpcClient, GlobalRpcClientAlreadySetError};

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

/// 初始化调用方传入的领域客户端。
///
/// 返回传入的 Bundle 列表（已完成客户端初始化），调用方应将其传给
/// [`crate::grpc::server_runner::start_grpc_server`] 以注册服务端。
///
/// # Arguments
///
/// - `outbound_service_key`：本服务对外服务级凭证（`cmx_sk_xxx`），由客户端出站时
///   注入到 gRPC metadata。`None` 表示未配置（兼容 loopback/单体场景）。
/// - `bundles`：组装层显式收集的领域 Bundle 列表（由皮肤 crate 提供，如
///   `cmx-orchestrator-rpc::OrchestratorBundle`）——**主应用提供哪些 RPC 服务的
///   唯一决定点**。
///
/// # Errors
///
/// - [`ClientInitError::Rpc`]：协议不是 `"grpc"`。
/// - [`ClientInitError::AlreadySet`]：重复初始化。
pub fn init_rpc_clients(
    config: &ServerConfig,
    cache: Arc<ServiceInstanceCache>,
    registry: Arc<dyn ServiceRegistry>,
    outbound_service_key: Option<String>,
    bundles: Vec<Box<dyn RpcServiceBundle>>,
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
        has_outbound_key = outbound_service_key.is_some(),
        "初始化 RPC 客户端（gRPC）"
    );

    let infra = Arc::new(
        GrpcInfrastructure::new(cache, config.grpc.clone(), registry)
            .with_outbound_service_key(outbound_service_key),
    );
    for b in &bundles {
        b.init_client(infra.clone());
    }
    GlobalRpcClient::mark_initialized()?;

    Ok(bundles)
}
