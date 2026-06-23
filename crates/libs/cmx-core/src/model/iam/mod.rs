//! IAM 基础数据模型（WASM 可见）
//!
//! 纯 Serialize/Deserialize 结构体，无 sqlx/modql 依赖。
//! WASM 插件可通过 cmx-core 感知调用者身份和权限信息。

pub mod user;
pub mod role;
pub mod permission;
pub mod permission_tree_node;
pub mod error;
pub mod registry;

pub use user::User;
pub use role::Role;
pub use permission::Permission;
pub use permission_tree_node::PermissionTreeNode;
pub use error::{PermissionDeniedError, RoleRequirement};
pub use registry::{
    PermissionInfo, PermissionRegistry, RegisteredPermission, RegisteredRouteHandler,
    all_registered_handlers, all_registered_permissions,
};
