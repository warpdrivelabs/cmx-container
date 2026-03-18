//! Domain 实体模块
//!
//! 包含 Domain 实体的完整定义，包括：
//! - DbBmc 实现（表元信息）
//! - Filter 定义（查询过滤）
//! - Entity 定义（实体结构）
//! - Service 实现（业务逻辑）
//! - Handler 实现（HTTP 处理）

mod bmc;
mod filter;
mod entity;
mod service;
mod handler;

pub use bmc::DomainBmc;
pub use filter::DomainFilter;
pub use entity::Domain;
pub use service::DomainService;
pub use handler::*;
