//! 权限管理模块

pub mod bmc;
pub mod entity;
pub mod filter;
pub mod service;

pub use bmc::PermissionBmc;
pub use entity::{PermissionForCreate, PermissionForUpdate};
pub use filter::PermissionFilter;
pub use service::PermissionServiceImpl;
