//! RPC 领域 trait 抽象。
//!
//! 定义跨实例 RPC 调用的统一接口，按领域拆分为：
//! - [`ServiceOrchestrationClient`]：服务编排（`call_service` / `call_function`）。
//! - [`PluginDataClient`]：插件数据管理（`import_plugin_data` / `cleanup_plugin_data`）。

pub mod error;
pub mod orchestrator;
pub mod plugin_data;
pub mod types;

pub use error::RpcError;
pub use orchestrator::ServiceOrchestrationClient;
pub use plugin_data::PluginDataClient;
pub use types::FunctionCallResult;
