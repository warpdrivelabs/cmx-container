//! 角色基础数据模型（WASM 可见）

use serde::{Deserialize, Serialize};
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

/// 角色基础数据模型（WASM 可见）。
///
/// 角色用于聚合一组权限并赋给用户，是 RBAC 模型中的核心实体。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct Role {
    /// 角色唯一标识（主键）。
    pub id: String,

    /// 角色编码，业务唯一，用于策略/权限绑定。
    pub code: String,

    /// 角色名称，用于展示。
    pub name: String,

    /// 数据权限范围（如 1 全部 / 2 本部门 / 3 本人 等），可空。
    #[serde(default)]
    pub data_scope: Option<i64>,

    /// 排序号，升序排列，可空。
    #[serde(default)]
    pub sort_order: Option<i64>,

    /// 状态（如 1 启用 / 0 禁用），可空。
    #[serde(default)]
    pub status: Option<i64>,

    /// 角色描述/备注，可空。
    #[serde(default)]
    pub description: Option<String>,

    /// 归档标记（如 1 已归档 / 0 正常），可空。
    #[serde(default)]
    pub archived: Option<i64>,

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
