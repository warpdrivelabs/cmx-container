//! cmx-orchestrator-rpc — 服务编排域的 gRPC 皮肤（薄 crate）。
//!
//! 提供服务编排域对外的 gRPC 能力三件套：
//! - client 访问器 [`orchestrator_client`]（`call_service` / `call_function`）；
//! - 服务端实现 [`CmxOrchestratorServerImpl`]（impl `CmxServiceOrchestrator`）；
//! - 装配 Bundle [`OrchestratorBundle`]（由组装层显式注册）。
//!
//! 依赖 cmx-rpc（共享设施）+ cmx-rpc-gen（proto 契约）+ cmx-traits（trait 抽象），
//! **不依赖业务 service crate**——业务实现经 `ServerDeps` 由组装层注入。
//!
//! 新增一个 gRPC 服务的标准步骤见 cmx-rpc/README.md。

mod client;
mod server;

pub use client::{OrchestratorBundle, OrchestratorGrpcClient, orchestrator_client};
pub use server::CmxOrchestratorServerImpl;
