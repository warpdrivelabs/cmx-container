//! RPC 领域 trait 抽象。
//!
//! 定义跨实例 RPC 调用的统一接口，按领域拆分为：
//! - [`ServiceOrchestrationClient`]：服务编排（`call_service` / `call_function`）。
//! - [`ResourceDataClient`]：资源数据管理（`import_resource_data` / `cleanup_resource_data`）。

pub mod error;
pub mod orchestrator;
pub mod resource_data;
pub mod types;

pub use error::RpcError;
pub use orchestrator::ServiceOrchestrationClient;
pub use resource_data::ResourceDataClient;
pub use types::FunctionCallResult;
