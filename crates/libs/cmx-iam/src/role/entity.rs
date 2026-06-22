//! 角色 Entity 定义

use modql::field::Fields;
use serde::{Deserialize, Serialize};
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

// Role 从 cmx-core re-export
pub use cmx_core::model::iam::Role;

/// 创建角色 DTO。
#[derive(Debug, Clone, Serialize, Deserialize, Fields)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct RoleForCreate {
    /// 角色编码，业务唯一，用于策略/权限绑定。
    pub code: String,

    /// 角色名称，用于展示。
    pub name: String,

    /// 数据权限范围（如 1 全部 / 2 本部门 / 3 本人 等）。
    #[serde(default)]
    pub data_scope: Option<i64>,

    /// 排序号，升序排列。
    #[serde(default)]
    pub sort_order: Option<i64>,

    /// 角色描述/备注。
    #[serde(default)]
    pub description: Option<String>,

    /// 父角色ID（NULL表示根角色，不支持权限继承，仅用于层级展示）
    #[serde(default)]
    pub parent_role_id: Option<String>,
}

/// 更新角色 DTO（全 `Option`，未提供字段不更新）。
#[derive(Debug, Clone, Serialize, Deserialize, Fields)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct RoleForUpdate {
    /// 角色名称。
    #[serde(default)]
    pub name: Option<String>,

    /// 数据权限范围。
    #[serde(default)]
    pub data_scope: Option<i64>,

    /// 排序号。
    #[serde(default)]
    pub sort_order: Option<i64>,

    /// 状态（1 启用 / 0 禁用）。
    #[serde(default)]
    pub status: Option<i64>,

    /// 角色描述/备注。
    #[serde(default)]
    pub description: Option<String>,

    /// 父角色ID（NULL表示根角色，不支持权限继承，仅用于层级展示）
    #[serde(default)]
    pub parent_role_id: Option<String>,
}

/// 分配权限请求（IAM 专用）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct AssignPermissionsRequest {
    /// 目标角色 ID。
    pub role_id: String,

    /// 待分配的权限 ID 列表（空数组表示清空所有权限）。
    pub permission_ids: Vec<String>,
}
