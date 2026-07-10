//! 角色管理模块

pub mod bmc;
pub mod entity;
pub mod filter;
pub mod service;

pub use bmc::{RoleBmc, RolePermissionBmc};
pub use entity::{
    AssignPermissionsRequest, AssignRoleUsersRequest, RoleForCreate, RoleForUpdate, RoleUserSummary,
};
pub use filter::RoleFilter;
pub use service::RoleServiceImpl;
