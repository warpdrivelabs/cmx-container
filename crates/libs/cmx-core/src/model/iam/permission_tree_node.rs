//! 权限树节点（WASM 可见）

use serde::{Deserialize, Serialize};
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

use super::Permission;

/// 权限树节点（WASM 可见）
///
/// 用于 get_permission_tree API 返回值，Permission + 递归 children 组合视图。
/// 定义在 cmx-core 而非 cmx-iam，因为 WASM SDK 需直接使用权限树结构。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct PermissionTreeNode {
    #[serde(flatten)]
    pub permission: Permission,
    pub children: Vec<PermissionTreeNode>,
}
