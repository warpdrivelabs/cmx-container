//! gRPC 服务启动器
//!
//! 封装 volo-grpc Server 的创建和启动逻辑。

use std::sync::Arc;

use cmx_rpc_gen::cmx::cmx_plugin_data_service::cmx_plugin_data_service::cmx as plugin_data_proto;
use cmx_rpc_gen::cmx::cmx_service_orchestrator::cmx_service_orchestrator::cmx::*;
use cmx_traits::plugin::PluginDataImporter;
use cmx_traits::plugin::PluginQuery;
use cmx_traits::runtime::RuntimeInvoker;
use cmx_traits::service::ServiceInvoker;
use tracing::instrument;
use volo::net::incoming::DefaultIncoming;
use volo_grpc::server::ServiceBuilder;

use crate::error::RpcFrameworkError;
use crate::server::CmxOrchestratorServiceImpl;

/// 启动 gRPC 服务
///
/// 监听指定端口，注册 CmxServiceOrchestrator 服务并运行。
/// 先绑定端口再发送就绪信号，避免启动竞态。
#[instrument(target = "cmx_rpc", skip(service_invoker, runtime_invoker, plugin_query, data_importer, ready_tx), fields(port = port))]
pub async fn start_grpc_server(
    port: u16,
    service_invoker: Arc<dyn ServiceInvoker>,
    runtime_invoker: Arc<dyn RuntimeInvoker>,
    plugin_query: Arc<dyn PluginQuery>,
    data_importer: Option<Arc<dyn PluginDataImporter>>,
    ready_tx: tokio::sync::oneshot::Sender<()>,
) -> Result<(), RpcFrameworkError> {
    let addr: std::net::SocketAddr = format!("[::]:{port}")
        .parse()
        .map_err(|e: std::net::AddrParseError| RpcFrameworkError::ServerStartFailed(e.to_string()))?;

    let service_impl = CmxOrchestratorServiceImpl::new(
        service_invoker,
        runtime_invoker,
        plugin_query,
        data_importer,
    );

    tracing::info!(
        target: "cmx_rpc",
        port = port,
        "启动 gRPC 服务"
    );

    // 注册服务编排服务
    let orchestrator_service = ServiceBuilder::new(CmxServiceOrchestratorServer::new(service_impl.clone()))
        .build::<CmxServiceOrchestratorRequestRecv, CmxServiceOrchestratorResponseSend>();

    // 注册插件数据导入服务
    let plugin_data_service = ServiceBuilder::new(plugin_data_proto::CmxPluginDataServiceServer::new(service_impl))
        .build::<plugin_data_proto::CmxPluginDataServiceRequestRecv, plugin_data_proto::CmxPluginDataServiceResponseSend>();

    // 先绑定端口，确保端口可用
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| RpcFrameworkError::ServerStartFailed(format!("端口绑定失败: {}", e)))?;

    tracing::info!(
        target: "cmx_rpc",
        port = port,
        local_addr = ?listener.local_addr(),
        "gRPC 端口绑定成功"
    );

    // 端口已绑定，发送就绪信号
    if ready_tx.send(()).is_err() {
        tracing::warn!(
            target: "cmx_rpc",
            port = port,
            "gRPC Server 就绪信号发送失败: 接收端已 drop（启动回调超时？）"
        );
    }

    // 使用预绑定的 listener 运行 Server
    let incoming = DefaultIncoming::from(listener);
    volo_grpc::server::Server::new()
        .add_service(orchestrator_service)
        .add_service(plugin_data_service)
        .run(incoming)
        .await
        .map_err(|e| RpcFrameworkError::ServerStartFailed(e.to_string()))?;

    Ok(())
}
