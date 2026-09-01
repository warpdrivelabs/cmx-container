//! gRPC 服务领域的 Bundle 模式，实现真正的开闭原则（OCP）。
//!
//! 每个领域封装为一个 [`RpcServiceBundle`]（皮肤 crate 提供，如
//! `cmx-orchestrator-rpc::OrchestratorBundle`），负责：
//! - 初始化该领域的客户端并注册到领域全局单例。
//! - 构建该领域的服务端注册闭包。
//!
//! 新增 gRPC 服务 = 在 `cmx-rpc-gen` 加 proto + 新建一个 `cmx-rpcs/*` 皮肤 crate
//! （含 Bundle 实现）+ 组装层（cmx-platform-app）Bundle 列表加一行。
//! `factory` / `server_runner` 零改动。
//!
//! # `ServerDeps` 耦合代价说明
//!
//! [`ServerDeps`] 含 4 个字段，每个 Bundle 都收到全量，但各领域按需取用、互不感知
//! （如 orchestrator 领域忽略 `data_importer`、resource_data 领域忽略前 2 个）。
//! 这是为换取 OCP（`factory`/`server_runner` 零改动）付出的合理耦合代价。当前仅
//! 2 领域，引入 `type Deps` 关联类型属过度设计（且与 `Box<dyn RpcServiceBundle>`
//! 对象化不兼容），本期不做。
//!
//! **演进路线**：当皮肤数量增长（≥5 域）且新增字段频繁时，可考虑把 `ServerDeps`
//! 改为按类型取用的容器（如 `HashMap<TypeId, Arc<dyn Any>>` + Bundle 内 downcast），
//! 使"新增一个领域的依赖"不再改动本文件、不触发其他皮肤 crate 重编。

use std::sync::Arc;

use crate::grpc::client::infra::GrpcInfrastructure;

/// 领域依赖（服务端组装用）。各 Bundle 按需取用，互不感知。
pub struct ServerDeps {
    /// 服务编排调用器（orchestrator 领域使用）。
    pub service_invoker: Arc<dyn cmx_traits::service::ServiceInvoker>,
    /// 插件函数调用器（orchestrator 领域使用；由组装层注入 cmx-biz 的实现，
    /// 封装 RuntimeInvoker + PluginQuery 完整调用链，使皮肤 crate 不直接依赖 cmx-biz）。
    pub function_invoker: Arc<dyn cmx_traits::function_invoker::FunctionInvoker>,
    /// 资源数据导入器（resource_data 领域使用，可选）。
    pub data_importer: Option<Arc<dyn cmx_traits::resource::ResourceDataImporter>>,
    /// 服务端鉴权器（各领域 server impl 在方法入口校验 gRPC 凭证）。
    /// `None` 表示不启用 gRPC 鉴权（兼容单体无 RPC 或 loopback 部署场景）。
    pub auth_verifier: Option<crate::grpc::server::AuthVerifier>,
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
        Self { inner: Box::new(f) }
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
