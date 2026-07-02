//! 服务领域 trait 抽象。
//!
//! 包含服务查询、服务存储、服务调用、全局服务调用器存储等接口。
//!
//! # 模块组织
//!
//! - [`query`] — 服务查询 trait（ServiceQuery）。
//! - [`storage`] — 服务存储 trait（ServiceStorage）。
//! - [`invoker`] — 服务调用 trait（ServiceInvoker）。
//! - [`global_invoker`] — 全局服务调用器存储器（GlobalServiceInvoker）。

pub mod global_invoker;
pub mod invoker;
pub mod query;
pub mod storage;

pub use global_invoker::{GlobalServiceInvoker, GlobalServiceInvokerError};
pub use invoker::{ServiceInvokeOptions, ServiceInvoker};
pub use query::{ServicePageFilter, ServicePageResult, ServiceQuery};
pub use storage::{SaveServiceVersionParams, ServiceStorage};
