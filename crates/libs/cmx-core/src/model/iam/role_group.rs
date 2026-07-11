//! 角色组基础数据模型（WASM 可见）

use serde::{Deserialize, Serialize};
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

/// 角色组基础数据模型（WASM 可见）。
///
/// 角色组用于将角色按业务维度分组，支持树形结构（通过 `parent_id` 关联）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct RoleGroup {
    /// 角色组唯一标识（主键）。
    pub id: String,

    /// 角色组名称，用于展示。
    pub name: String,

    /// 父角色组 ID，根节点为 `None`，可空。
    #[serde(default)]
    pub parent_id: Option<String>,

    /// 排序号，升序排列，可空。
    #[serde(default)]
    pub sort_order: Option<i64>,

    /// 角色组描述/备注，可空。
    #[serde(default)]
    pub description: Option<String>,

    /// 归档标记（如 1 已归档 / 0 正常），可空。
    #[serde(default)]
    pub archived: Option<i64>,

    /// 状态（1 启用 / 0 停用）。
    #[serde(default)]
    pub status: Option<i64>,

    /// 创建时间（ISO8601 字符串），可空。
    #[serde(default)]
    pub create_time: Option<String>,

    /// 更新时间（ISO8601 字符串），可空。
    #[serde(default)]
    pub update_time: Option<String>,

    /// 创建人 ID，可空。
    #[serde(default)]
    pub create_by: Option<String>,

    /// 创建人姓名，可空。
    #[serde(default)]
    pub create_name: Option<String>,

    /// 更新人 ID，可空。
    #[serde(default)]
    pub update_by: Option<String>,

    /// 更新人姓名，可空。
    #[serde(default)]
    pub update_name: Option<String>,
}
