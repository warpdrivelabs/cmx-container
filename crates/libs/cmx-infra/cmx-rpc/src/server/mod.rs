//! gRPC 服务端模块。
//!
//! 按领域拆分为：
//! - [`orchestrator`]：服务编排服务端（实现 `CmxServiceOrchestrator`）。
//! - [`resource_data`]：资源数据管理服务端（实现 `CmxResourceDataService`）。

pub mod orchestrator;
pub mod resource_data;

pub use orchestrator::CmxOrchestratorServerImpl;
pub use resource_data::CmxResourceDataServerImpl;
