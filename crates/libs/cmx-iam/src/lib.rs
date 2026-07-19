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
// API Key / OAuth2 客户端数据服务（cmx_auth_* 物理表读写下沉，供 cmx-api 认证 handler 调用）。
pub mod api_key;
pub mod error;
pub mod host_functions;
pub mod iam_checker;
pub mod oauth_client;
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
pub use cmx_core::model::iam::{
    Permission, PermissionTreeNode, Role, RoleGroup, RoleGroupTreeNode, User,
};

// Re-export WASM 宿主函数提供者
pub use host_functions::IamHostFunctions;

// Re-export 本 crate 类型
pub use config::IamConfig;
pub use error::IamError;
pub use iam_checker::IamChecker;
// ResourceDataImporterImpl 已迁移至 cmx-biz(多类别路由器不属于权限域)
pub use role_group::RoleGroupServiceImpl;
pub use rule::{ExclusionRuleServiceImpl, RuleEnforcer, RuleEnforcerImpl};
pub use service_traits::{
    PermissionService, RoleGroupService, RoleService, TempAssignmentStatusFilter,
    UserRoleAssignment, UserService,
};
pub use user_auth_query_impl::UserAuthQueryImpl;
