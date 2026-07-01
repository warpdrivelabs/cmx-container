//! IAM Service trait 定义（cmx-iam 内部）。
//!
//! 按子模块组织各服务 trait：
//! - `user`：`UserService` trait 及用户专属审计响应结构体。
//! - `role`：`RoleService` trait 及角色专属审计响应结构体。
//! - `role_group`：`RoleGroupService` trait。
//! - `permission`：`PermissionService` trait 及权限专属审计响应结构体。
//! - `audit`：跨模块复用的审计摘要结构体（`RoleSummary` / `PermissionSummary`）。
//!
//! 各 trait 的具体实现位于对应子模块的 `service.rs` 中。
//! 本模块通过 `pub use` 统一 re-export 所有 trait 与类型，保持对外 API 兼容。

pub mod audit;
pub mod permission;
pub mod role;
pub mod role_group;
pub mod user;

pub use audit::{PermissionSummary, RoleSummary};
pub use permission::{PermissionService, PermissionUsageStat};
pub use role::{PermissionDiffResponse, RoleService};
pub use role_group::RoleGroupService;
pub use user::{
    EffectivePermissionsResponse, TempAssignmentStatusFilter, UserRoleAssignment, UserService,
};
