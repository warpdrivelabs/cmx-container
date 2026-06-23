//! 权限基础数据模型（WASM 可见）

use serde::{Deserialize, Serialize};
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

/// 权限基础数据模型（WASM 可见）。
///
/// 权限描述系统中的一个可执行操作（如菜单、按钮、API 资源），
/// 支持树形结构（通过 `parent_id` 关联）和资源类型分类。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct Permission {
    /// 权限唯一标识（主键）。
    pub id: String,

    /// 权限编码（如 `system:user:add`），业务唯一，用于鉴权匹配。
    pub code: String,

    /// 权限名称，用于展示。
    pub name: String,

    /// 资源类型（如 `menu` / `button` / `api`），可空。
    #[serde(default)]
    pub resource_type: Option<String>,

    /// 父权限 ID，根权限为 `None`，可空。
    #[serde(default)]
    pub parent_id: Option<String>,

    /// 排序号，升序排列，可空。
    #[serde(default)]
    pub sort_order: Option<i64>,

    /// 状态（如 1 启用 / 0 禁用），可空。
    #[serde(default)]
    pub status: Option<i64>,

    /// 权限描述/备注，可空。
    #[serde(default)]
    pub description: Option<String>,

    /// 所属域编码（如 `platform`、`tenant`），可空。
    #[serde(default)]
    pub domain_code: Option<String>,

    /// 所属应用编码（如 `user-center`、`billing`），可空。
    #[serde(default)]
    pub app_code: Option<String>,

    /// 所属模块编码（如 `user`、`order`），可空。
    #[serde(default)]
    pub module_code: Option<String>,

    /// 扩展配置（用户自定义 JSON 文本），可空。
    #[serde(default)]
    pub extension: Option<String>,

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
