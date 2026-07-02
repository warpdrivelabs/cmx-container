//! gRPC 服务启动器。
//!
//! 封装 volo-grpc [`Server`](volo_grpc::server::Server) 的创建和启动逻辑。
//! 迭代 [`RpcServiceBundle`] 列表，每个 Bundle 把自己的 service 加到 server 上（OCP）。

use tracing::instrument;
use volo::net::incoming::DefaultIncoming;

use crate::bundle::{RpcServiceBundle, ServerDeps};
use crate::error::RpcFrameworkError;

/// 启动 gRPC 服务。
///
/// 监听指定端口，迭代 `bundles` 注册各领域的 service 并运行。
/// 先绑定端口再发送就绪信号，避免启动竞态。
///
/// # Arguments
///
/// * `port` - gRPC 监听端口。
/// * `bundles` - 已完成客户端初始化的 Bundle 列表（由 [`crate::factory::init_rpc_clients`] 返回）。
/// * `deps` - 服务端依赖（各 Bundle 按需取用）。
/// * `ready_tx` - 就绪信号发送端，端口绑定成功后发送。
///
/// # Errors
///
/// - [`RpcFrameworkError::ServerStartFailed`]：端口绑定或运行失败。
#[instrument(
    target = "cmx_rpc",
    skip(bundles, deps, ready_tx),
    fields(port = port)
)]
pub async fn start_grpc_server(
    port: u16,
    bundles: Vec<Box<dyn RpcServiceBundle>>,
    deps: ServerDeps,
    ready_tx: tokio::sync::oneshot::Sender<()>,
) -> Result<(), RpcFrameworkError> {
    let addr: std::net::SocketAddr =
        format!("[::]:{port}")
            .parse()
            .map_err(|e: std::net::AddrParseError| {
                RpcFrameworkError::ServerStartFailed(e.to_string())
            })?;

    // OCP：fold 迭代 bundles，每个 bundle 把自己的 service 加到 server
    let server = bundles
        .into_iter()
        .fold(volo_grpc::server::Server::new(), |server, bundle| {
            bundle.build_server(&deps).apply(server)
        });

    tracing::info!(target: "cmx_rpc", port = port, "启动 gRPC 服务");

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
    server
        .run(incoming)
        .await
        .map_err(|e| RpcFrameworkError::ServerStartFailed(e.to_string()))?;

    Ok(())
}
