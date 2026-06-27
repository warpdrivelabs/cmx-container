//! gRPC 服务领域的 Bundle 模式，实现真正的开闭原则（OCP）。
//!
//! 每个领域封装为一个 [`RpcServiceBundle`]，负责：
//! - 初始化该领域的客户端并注册到领域全局单例。
//! - 构建该领域的服务端注册闭包。
//!
//! 新增 gRPC 服务 = 新增一个领域模块（含 Bundle 实现）+ 在 [`default_bundles()`] 加一行。
//! `factory` / `global` / `server_runner` 零改动。
//!
//! # `ServerDeps` 耦合代价说明
//!
//! [`ServerDeps`] 含 3 个字段，每个 Bundle 都收到全量，但 `OrchestratorBundle` 忽略
//! `data_importer`、`PluginDataBundle` 忽略前 2 个。这是为换取 OCP
//! （`factory`/`server_runner` 零改动）付出的合理耦合代价。当前仅 2 领域，引入
//! `type Deps` 关联类型属过度设计，本期不做；若未来 Bundle 数量增长，可再考虑每 Bundle
//! 自带关联类型。

use std::sync::Arc;

use crate::client::infra::GrpcInfrastructure;

/// 领域依赖（服务端组装用）。各 Bundle 按需取用，互不感知。
pub struct ServerDeps {
    /// 服务编排调用器（orchestrator 领域使用）。
    pub service_invoker: Arc<dyn cmx_traits::service::ServiceInvoker>,
    /// 插件函数调用器（orchestrator 领域使用；由组装层注入 cmx-biz 的实现，
    /// 封装 RuntimeInvoker + PluginQuery 完整调用链，使 cmx-rpc 不直接依赖 cmx-biz）。
    pub function_invoker: Arc<dyn cmx_traits::function_invoker::FunctionInvoker>,
    /// 插件数据导入器（plugin_data 领域使用，可选）。
    pub data_importer: Option<Arc<dyn cmx_traits::plugin::PluginDataImporter>>,
}

/// "把 service 加到 server 上"的类型擦除闭包。
///
/// 因 volo-grpc [`volo_grpc::server::Server::add_service`] 接收任意 service 类型
/// （返回 `Self`，服务存入内部 `Router` 字段而非类型参数），用 `FnOnce` 闭包在 Bundle
/// 内部 monomorphize，对外类型擦除为 `Box<dyn FnOnce(Server) -> Server + Send>`。
pub struct ServerRegistration {
    inner: Box<dyn FnOnce(volo_grpc::server::Server) -> volo_grpc::server::Server + Send>,
}

impl ServerRegistration {
    /// 创建新的服务端注册闭包。
    pub fn new<F>(f: F) -> Self
    where
        F: FnOnce(volo_grpc::server::Server) -> volo_grpc::server::Server + Send + 'static,
    {
        Self {
            inner: Box::new(f),
        }
    }

    /// 把服务加到 server 上。
    pub fn apply(self, server: volo_grpc::server::Server) -> volo_grpc::server::Server {
        (self.inner)(server)
    }
}

/// 领域 Bundle 接口。
pub trait RpcServiceBundle: Send + Sync {
    /// 领域名（日志/诊断用）。
    fn name(&self) -> &'static str;
    /// 初始化客户端：构建并注册到该领域全局单例。
    fn init_client(&self, infra: Arc<GrpcInfrastructure>);
    /// 构建服务端注册闭包。
    fn build_server(&self, deps: &ServerDeps) -> ServerRegistration;
}

/// 内置 Bundle 清单（新增领域时此处加一行）。
pub fn default_bundles() -> Vec<Box<dyn RpcServiceBundle>> {
    vec![
        Box::new(crate::client::orchestrator::OrchestratorBundle),
        Box::new(crate::client::plugin_data::PluginDataBundle),
    ]
}
