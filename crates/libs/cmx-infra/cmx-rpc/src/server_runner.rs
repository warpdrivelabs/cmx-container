//! gRPC 服务启动器
//!
//! 封装 volo-grpc Server 的创建和启动逻辑。

use std::sync::Arc;

use cmx_rpc_gen::cmx::cmx_service_orchestrator::cmx_service_orchestrator::cmx::*;
use cmx_traits::{RuntimeInvoker, ServiceInvoker};
use tracing::instrument;
use volo::net::Address;
use volo_grpc::server::ServiceBuilder;

use crate::error::RpcFrameworkError;
use crate::server::CmxOrchestratorServiceImpl;

/// 启动 gRPC 服务
///
/// 监听指定端口，注册 CmxServiceOrchestrator 服务并运行。
#[instrument(target = "cmx_rpc", skip(service_invoker, runtime_invoker), fields(port = port))]
pub async fn start_grpc_server(
    port: u16,
    service_invoker: Arc<dyn ServiceInvoker>,
    runtime_invoker: Arc<dyn RuntimeInvoker>,
) -> Result<(), RpcFrameworkError> {
    let addr: std::net::SocketAddr = format!("[::]:{port}")
        .parse()
        .map_err(|e: std::net::AddrParseError| RpcFrameworkError::ServerStartFailed(e.to_string()))?;

    let service_impl = CmxOrchestratorServiceImpl::new(service_invoker, runtime_invoker);

    tracing::info!(
        target: "cmx_rpc",
        port = port,
        "启动 gRPC 服务"
    );

    let service = ServiceBuilder::new(CmxServiceOrchestratorServer::new(service_impl))
        .build::<CmxServiceOrchestratorRequestRecv, CmxServiceOrchestratorResponseSend>();

    volo_grpc::server::Server::new()
        .add_service(service)
        .run(Address::Ip(addr))
        .await
        .map_err(|e| RpcFrameworkError::ServerStartFailed(e.to_string()))?;

    Ok(())
}
