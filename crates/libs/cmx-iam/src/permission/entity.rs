//! 权限 Entity 定义

use modql::field::Fields;
use serde::{Deserialize, Serialize};
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

// Permission 从 cmx-core re-export
pub use cmx_core::model::iam::Permission;

/// 创建权限 DTO。
#[derive(Debug, Clone, Serialize, Deserialize, Fields)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct PermissionForCreate {
    /// 权限编码（如 `system:user:add`），业务唯一，用于鉴权匹配。
    pub code: String,

    /// 权限名称，用于展示。
    pub name: String,

    /// 资源类型（如 `menu` / `button` / `api`）。
    #[serde(default)]
    pub resource_type: Option<String>,

    /// 父权限 ID，根权限为 `None`。
    #[serde(default)]
    pub parent_id: Option<String>,

    /// 排序号，升序排列。
    #[serde(default)]
    pub sort_order: Option<i64>,

    /// 权限描述/备注。
    #[serde(default)]
    pub description: Option<String>,

    /// 所属域编码（如 `platform`、`tenant`）。
    #[serde(default)]
    pub domain_code: Option<String>,

    /// 所属应用编码（如 `user-center`、`billing`）。
    #[serde(default)]
    pub app_code: Option<String>,

    /// 所属模块编码（如 `user`、`order`）。
    #[serde(default)]
    pub module_code: Option<String>,

    /// 扩展配置（用户自定义 JSON 文本）。
    #[serde(default)]
    pub extension: Option<String>,
}

/// 阻止删除的角色信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct BlockedRoleInfo {
    /// 角色 ID。
    pub role_id: String,
    /// 角色编码。
    pub role_code: String,
    /// 角色名称。
    pub role_name: String,
}

/// 单个权限被阻止删除的详情（含其关联的角色列表）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct BlockedPermissionInfo {
    /// 权限 ID。
    pub permission_id: String,
    /// 权限编码。
    pub permission_code: String,
    /// 权限名称。
    pub permission_name: String,
    /// 关联的角色列表。
    pub roles: Vec<BlockedRoleInfo>,
}

/// 删除权限的整体阻止详情。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct DeletePermissionBlocked {
    /// 被阻止的权限及其角色列表。
    pub blocked_permissions: Vec<BlockedPermissionInfo>,
}

/// 删除权限成功结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct DeletePermissionResult {
    /// 实际删除的权限 ID 列表（含级联删除的子权限）。
    pub deleted_permission_ids: Vec<String>,
    /// 删除的权限数量。
    pub deleted_count: u64,
}

/// 删除权限结果枚举（成功 | 被阻止）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(tag = "outcome")]
pub enum DeletePermissionOutcome {
    /// 删除成功。
    Deleted {
        /// 删除结果。
        result: DeletePermissionResult,
    },
    /// 被角色使用，阻止删除。
    Blocked {
        /// 阻止详情。
        detail: DeletePermissionBlocked,
    },
}

/// 更新权限 DTO（全 `Option`，未提供字段不更新）。
#[derive(Debug, Clone, Serialize, Deserialize, Fields)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct PermissionForUpdate {
    /// 权限名称。
    #[serde(default)]
    pub name: Option<String>,

    /// 资源类型。
    #[serde(default)]
    pub resource_type: Option<String>,

    /// 父权限 ID。
    #[serde(default)]
    pub parent_id: Option<String>,

    /// 排序号。
    #[serde(default)]
    pub sort_order: Option<i64>,

    /// 状态（1 启用 / 0 禁用）。
    #[serde(default)]
    pub status: Option<i64>,

    /// 权限描述/备注。
    #[serde(default)]
    pub description: Option<String>,

    /// 所属域编码。
    #[serde(default)]
    pub domain_code: Option<String>,

    /// 所属应用编码。
    #[serde(default)]
    pub app_code: Option<String>,

    /// 所属模块编码。
    #[serde(default)]
    pub module_code: Option<String>,

    /// 扩展配置（用户自定义 JSON 文本）。
    #[serde(default)]
    pub extension: Option<String>,
}
