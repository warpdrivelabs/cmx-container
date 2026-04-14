//! 编排器模块
//!
//! 基于 Flow JSON 的 DAG 编排执行引擎，支持：
//! - 线性流程执行：start -> func -> func -> end
//! - 事务框支持：多个函数在同一个数据库事务中执行
//! - 多分支路由：switch 节点根据返回值选择执行路径
//! - SVRContext 上下文传递：初始入参、请求头、各步骤输出在函数间传递
//!
//! 模块结构：
//! - `types` - 类型定义（OrchestrationResult, ExecutionStep, ExecuteOptions 等）
//! - `executor` - 编排执行器（Orchestrator 主执行逻辑）
//! - `node_handler` - 节点执行器（统一 func/switch 的 WASM 调用逻辑）
//! - `flow_navigator` - 流程导航器（节点和边的查找）
//! - `transaction_manager` - 事务管理器（事务状态跟踪和生命周期管理）

mod types;
mod executor;
mod node_handler;
mod flow_navigator;
mod transaction_manager;

pub use types::*;
pub use executor::Orchestrator;
