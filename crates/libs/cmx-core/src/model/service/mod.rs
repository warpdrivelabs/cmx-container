//! 服务模型模块
//!
//! 包含服务编排相关的所有结构体，按职责分为：
//! - `definition.rs`: 服务定义
//! - `orchestration.rs`: 服务编排定义
//! - `flow.rs`: 流程结构（节点、边）
//! - `context.rs`: 运行时上下文
//! - `wasm_io.rs`: WASM 输入输出

pub mod definition;
pub mod orchestration;
pub mod flow;
pub mod context;
pub mod wasm_io;

pub use definition::ServiceDefinition;
pub use orchestration::ServiceOrchestration;
pub use flow::{ServiceFlow, ServiceNode, ServiceEdge, NodeMeta, NodeSize, NodePosition, NodeData, NodeNodeMeta};
pub use context::SVRContext;
pub use context::AuthContext;
pub use wasm_io::{FunctionInput, FunctionOutput};
