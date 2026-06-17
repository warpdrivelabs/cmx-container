//! 角色管理模块

pub mod bmc;
pub mod entity;
pub mod filter;
pub mod service;

pub use bmc::{RoleBmc, RolePermissionBmc};
pub use entity::{AssignPermissionsRequest, RoleForCreate, RoleForUpdate};
pub use filter::RoleFilter;
pub use service::RoleServiceImpl;
