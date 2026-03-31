//! Domain 模块
//!
//! 提供领域实体的 CRUD 操作

mod bmc;
mod entity;
mod filter;
pub mod handler;
mod service;

pub use bmc::DomainBmc;
pub use entity::{Domain, DomainForCreate, DomainForUpdate, DomainTreeNodeData};
pub use filter::DomainFilter;
pub use service::DomainService;
