//! gRPC 服务端共享设施。
//!
//! - [`auth_layer`]：服务端鉴权（`AuthVerifier` + `verify_request`）。
//!
//! 具体领域的 gRPC 服务端实现（orchestrator / resource_data 等）已迁至 `cmx-rpcs/*`
//! 皮肤 crate（如 `cmx-orchestrator-rpc`），经本模块鉴权设施做入口校验。

pub mod auth_layer;

pub use auth_layer::{AuthVerifier, VerifiedAuth, verify_request};
