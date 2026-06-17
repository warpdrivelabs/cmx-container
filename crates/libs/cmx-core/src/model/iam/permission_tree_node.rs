//! 权限树节点（WASM 可见）

use serde::{Deserialize, Serialize};
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

use super::Permission;

/// 权限树节点（WASM 可见）。
///
/// 用于 `get_permission_tree` API 返回值，`Permission` 与递归 `children` 组合视图。
/// 定义在 `cmx-core` 而非 `cmx-iam`，因为 WASM SDK 需直接使用权限树结构。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct PermissionTreeNode {
    /// 当前节点对应的权限详情，通过 `#[serde(flatten)]` 在序列化时平铺为同级字段。
    #[serde(flatten)]
    pub permission: Permission,

    /// 子节点列表，递归引用自身构成树形结构；`openapi` 模式下禁用递归类型展开。
    #[cfg_attr(feature = "openapi", schema(no_recursion))]
    pub children: Vec<PermissionTreeNode>,
}
