//! cmx-iam — 用户权限角色管理（IAM）业务 crate
//!
//! 提供 User/Role/Permission 的 Entity/BMC/Filter 定义，
//! UserAuthQuery trait 实现，以及 Service 层业务逻辑。
//!
//! 本 crate 为服务端专用，WASM 不可达。
//! 基础数据模型（User/Role/Permission）定义在 cmx-core 中。

pub mod audit_helper;
pub mod circuit_breaker;
pub mod config;
pub mod error;
pub mod iam_checker;
pub mod permission;
pub mod role;
pub mod role_group;
pub mod rule;
pub mod scheduler;
pub mod service_traits;
pub mod traits;
pub mod user;
pub mod user_auth_query_impl;

// Re-export cmx-core 基础数据模型
pub use cmx_core::model::iam::{Permission, PermissionTreeNode, Role, RoleGroup, RoleGroupTreeNode, User};

// Re-export 本 crate 类型
pub use config::IamConfig;
pub use error::IamError;
pub use iam_checker::IamChecker;
pub use permission::PluginDataImporterImpl;
pub use role_group::RoleGroupServiceImpl;
pub use rule::{ExclusionRuleServiceImpl, RuleEnforcer, RuleEnforcerImpl};
pub use service_traits::{
    PermissionService, RoleGroupService, RoleService, TempAssignmentStatusFilter, UserService, UserRoleAssignment,
};
pub use user_auth_query_impl::UserAuthQueryImpl;
