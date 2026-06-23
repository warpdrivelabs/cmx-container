//! 权限管理模块

pub mod bmc;
pub mod consistency_check;
pub mod entity;
pub mod filter;
pub mod service;

pub use bmc::PermissionBmc;
pub use consistency_check::{
    run_consistency_check, warn_handler_annotation_status, log_registered_permissions,
    ConsistencyReport,
};
pub use entity::{PermissionForCreate, PermissionForUpdate};
pub use filter::PermissionFilter;
pub use service::PermissionServiceImpl;
