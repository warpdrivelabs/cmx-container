//! IAM Service trait 定义（已拆分至 `traits` 子模块）。
//!
//! 本文件保留为向后兼容的 re-export 入口，所有 trait 与类型已迁移至 `crate::traits`：
//! - `UserService` / `TempAssignmentStatusFilter` / `UserRoleAssignment` /
//!   `EffectivePermissionsResponse` → `crate::traits::user`
//! - `RoleService` / `PermissionDiffResponse` → `crate::traits::role`
//! - `RoleGroupService` → `crate::traits::role_group`
//! - `PermissionService` / `PermissionUsageStat` → `crate::traits::permission`
//! - `RoleSummary` / `PermissionSummary` → `crate::traits::audit`
//!
//! 新代码请直接使用 `crate::traits::` 路径。

pub use crate::traits::*;
