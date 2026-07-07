//! gRPC 服务端模块。
//!
//! 按领域拆分为：
//! - [`auth_layer`]：服务端鉴权（`AuthVerifier` + `verify_request`）。
//! - [`orchestrator`]：服务编排服务端（实现 `CmxServiceOrchestrator`）。
//! - [`resource_data`]：资源数据管理服务端（实现 `CmxResourceDataService`）。

pub mod auth_layer;
pub mod orchestrator;
pub mod resource_data;

pub use auth_layer::{AuthVerifier, VerifiedAuth, verify_request};
pub use orchestrator::CmxOrchestratorServerImpl;
pub use resource_data::CmxResourceDataServerImpl;
