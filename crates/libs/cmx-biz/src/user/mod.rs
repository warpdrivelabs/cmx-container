//! 用户权限角色管理模块
//!
//! 提供 User/Role/Permission 的 Entity/Bmc/Filter/Service 定义，
//! 以及 UserAuthQuery trait 的实现。

pub mod auth_query;
pub mod bmc;
pub mod entity;
pub mod filter;
pub mod service;

pub use auth_query::UserAuthQueryImpl;
pub use bmc::{PermissionBmc, RoleBmc, RolePermissionBmc, UserBmc, UserRoleBmc};
pub use entity::{
    Permission, PermissionForCreate, Role, RoleForCreate, User, UserForCreate, UserForUpdate,
};
pub use filter::{PermissionFilter, RoleFilter, UserFilter};
pub use service::UserService;
