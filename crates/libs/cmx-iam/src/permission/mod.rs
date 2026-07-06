//! 权限管理模块

pub mod bmc;
pub mod consistency_check;
pub mod entity;
pub mod filter;
pub mod service;
pub mod zip_importer;

pub use bmc::PermissionBmc;
pub use consistency_check::{
    ConsistencyReport, log_registered_permissions, run_consistency_check,
    warn_handler_annotation_status,
};
pub use entity::{
    BlockedPermissionInfo, BlockedRoleInfo, DeletePermissionBlocked, DeletePermissionOutcome,
    DeletePermissionResult, PermissionForCreate, PermissionForUpdate,
};
pub use filter::PermissionFilter;
pub use service::{PermissionDefinition, PermissionFile, PermissionServiceImpl};
// zip_importer 模块为 PermissionServiceImpl 实现 PermissionZipImporter trait,
// 无需 re-export 类型(trait impl 在 trait 处于 scope 时自动可用)
// ResourceDataImporterImpl 已迁移至 cmx-biz(多类别路由器不属于权限域)
