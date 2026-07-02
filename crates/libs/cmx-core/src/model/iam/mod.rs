//! IAM 基础数据模型（WASM 可见）
//!
//! 纯 Serialize/Deserialize 结构体，无 sqlx/modql 依赖。
//! WASM 插件可通过 cmx-core 感知调用者身份和权限信息。

pub mod error;
pub mod permission;
pub mod permission_tree_node;
pub mod registry;
pub mod role;
pub mod role_group;
pub mod role_group_tree_node;
pub mod user;

pub use error::{PermissionDeniedError, RoleRequirement};
pub use permission::Permission;
pub use permission_tree_node::PermissionTreeNode;
pub use registry::{
    PermissionInfo, PermissionRegistry, RegisteredPermission, RegisteredRouteHandler,
    all_registered_handlers, all_registered_permissions,
};
pub use role::Role;
pub use role_group::RoleGroup;
pub use role_group_tree_node::RoleGroupTreeNode;
pub use user::User;
