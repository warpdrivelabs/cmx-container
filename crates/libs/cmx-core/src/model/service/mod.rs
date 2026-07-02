//! 服务模型模块
//!
//! 包含服务编排相关的所有结构体，按职责分为：
//! - `definition.rs`: 服务定义
//! - `orchestration.rs`: 服务编排定义
//! - `flow.rs`: 流程结构（节点、边）
//! - `context.rs`: 运行时上下文
//! - `wasm_io.rs`: WASM 输入输出

pub mod context;
pub mod definition;
pub mod flow;
pub mod orchestration;
pub mod wasm_io;

pub use context::AuthContext;
pub use context::SVRContext;
pub use definition::ServiceDefinition;
pub use flow::{
    NodeData, NodeMeta, NodeNodeMeta, NodePosition, NodeSize, ServiceEdge, ServiceFlow, ServiceNode,
};
pub use orchestration::ServiceOrchestration;
pub use wasm_io::{FunctionInput, FunctionOutput};
