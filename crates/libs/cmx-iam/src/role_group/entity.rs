//! 角色组 Entity 定义

use modql::field::Fields;
use serde::{Deserialize, Serialize};
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

// RoleGroup 从 cmx-core re-export
pub use cmx_core::model::iam::RoleGroup;

/// 创建角色组 DTO。
#[derive(Debug, Clone, Serialize, Deserialize, Fields)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct RoleGroupForCreate {
    /// 角色组名称，用于展示。
    pub name: String,

    /// 父角色组 ID，根节点为 `None`。
    #[serde(default)]
    pub parent_id: Option<String>,

    /// 排序号，升序排列。
    #[serde(default)]
    pub sort_order: Option<i64>,

    /// 角色组描述/备注。
    #[serde(default)]
    pub description: Option<String>,
}

/// 更新角色组 DTO（全 `Option`，未提供字段不更新）。
#[derive(Debug, Clone, Serialize, Deserialize, Fields)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct RoleGroupForUpdate {
    /// 角色组名称。
    #[serde(default)]
    pub name: Option<String>,

    /// 父角色组 ID。
    #[serde(default)]
    pub parent_id: Option<String>,

    /// 排序号。
    #[serde(default)]
    pub sort_order: Option<i64>,

    /// 角色组描述/备注。
    #[serde(default)]
    pub description: Option<String>,
}
