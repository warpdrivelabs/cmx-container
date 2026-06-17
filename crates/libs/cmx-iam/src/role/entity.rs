//! 角色 Entity 定义

use modql::field::Fields;
use serde::{Deserialize, Serialize};
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

// Role 从 cmx-core re-export
pub use cmx_core::model::iam::Role;

/// 创建角色 DTO
#[derive(Debug, Clone, Serialize, Deserialize, Fields)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct RoleForCreate {
    pub code: String,
    pub name: String,
    #[serde(default)]
    pub data_scope: Option<i64>,
    #[serde(default)]
    pub sort_order: Option<i64>,
    #[serde(default)]
    pub description: Option<String>,
}

/// 更新角色 DTO
#[derive(Debug, Clone, Serialize, Deserialize, Fields)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct RoleForUpdate {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub data_scope: Option<i64>,
    #[serde(default)]
    pub sort_order: Option<i64>,
    #[serde(default)]
    pub status: Option<i64>,
    #[serde(default)]
    pub description: Option<String>,
}

/// 分配权限请求（IAM 专用）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct AssignPermissionsRequest {
    pub role_id: String,
    pub permission_ids: Vec<String>,
}
