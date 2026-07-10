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

    /// 所属角色组 ID，未分组为 None。
    #[serde(default)]
    pub role_group_id: Option<String>,

    /// 数据权限范围（如 1 全部 / 2 本部门 / 3 本人 等）。
    #[serde(default)]
    pub data_scope: Option<i64>,

    /// 排序号，升序排列。
    #[serde(default)]
    pub sort_order: Option<i64>,

    /// 角色描述/备注。
    #[serde(default)]
    pub description: Option<String>,
}

/// 更新角色 DTO（全 `Option`，未提供字段不更新）。
#[derive(Debug, Clone, Serialize, Deserialize, Fields)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct RoleForUpdate {
    /// 角色名称。
    #[serde(default)]
    pub name: Option<String>,

    /// 所属角色组 ID。
    #[serde(default)]
    pub role_group_id: Option<String>,

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

/// 批量给角色分配用户请求（全量替换）。
///
/// 将目标角色的用户集合设置为 `user_ids`，原有不在列表中的用户关联会被移除。
/// 空数组表示清空该角色的所有用户。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct AssignRoleUsersRequest {
    /// 目标角色 ID。
    pub role_id: String,

    /// 待分配的用户 ID 列表（全量替换语义）。
    pub user_ids: Vec<String>,
}

/// 角色下永久授权用户的精简投影（不含密码等敏感字段）。
///
/// 仅用于查询返回，不是 CRUD 实体，故不 derive Fields。
/// 通过 `get_role_users` 单次 JOIN 查询直接构造。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct RoleUserSummary {
    /// 用户 ID。
    pub user_id: String,

    /// 用户名（唯一）。
    pub username: String,

    /// 昵称，可空。
    pub nickname: Option<String>,
}
