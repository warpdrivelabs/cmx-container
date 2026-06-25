//! gRPC 服务端模块。
//!
//! 按领域拆分为：
//! - [`orchestrator`]：服务编排服务端（实现 `CmxServiceOrchestrator`）。
//! - [`plugin_data`]：插件数据管理服务端（实现 `CmxPluginDataService`）。

pub mod orchestrator;
pub mod plugin_data;

pub use orchestrator::CmxOrchestratorServerImpl;
pub use plugin_data::CmxPluginDataServerImpl;
