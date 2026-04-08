//! cmx-service — 企业级通用服务层
//!
//! 作为插件编排的执行引擎，协调 PluginQuery 和 RuntimeInvoker 完成请求处理。
//! 提供：
//! - CmxService 核心服务结构
//! - Orchestrator 编排执行器
//! - ServiceHandler HTTP 处理器
//!
//! # 依赖关系
//!
//! - 依赖 cmx-traits（trait 定义）
//! - 依赖 cmx-database（直接执行 SQL）
//! - **不依赖** cmx-plugin（通过 PluginQuery trait 交互）

pub mod error;
pub mod handler;
pub mod orchestrator;
pub mod request;
pub mod service;

pub use error::ServiceError;
pub use handler::ServiceHandler;
pub use orchestrator::{Orchestration, OrchestrationResult, Orchestrator, OrchestrationStep, StepInput};
pub use request::{InvokeRequest, InvokeResponse, OrchestrateRequest, OrchestrateResponse, StepResult};
pub use service::{CmxService, ServiceConfig};
