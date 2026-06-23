//! 角色组树节点（WASM 可见）

use serde::{Deserialize, Serialize};
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

use super::RoleGroup;

/// 角色组树节点（WASM 可见）。
///
/// 用于 `get_role_group_tree` API 返回值，`RoleGroup` 与递归 `children` 组合视图。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct RoleGroupTreeNode {
    /// 当前节点对应的角色组详情，通过 `#[serde(flatten)]` 在序列化时平铺为同级字段。
    #[serde(flatten)]
    pub role_group: RoleGroup,

    /// 子节点列表，递归引用自身构成树形结构；`openapi` 模式下禁用递归类型展开。
    #[cfg_attr(feature = "openapi", schema(no_recursion))]
    pub children: Vec<RoleGroupTreeNode>,
}
