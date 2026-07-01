//! Menu 实体定义
//!
//! 定义 Menu 实体的数据结构，包括完整实体和创建/更新 DTO。
//! 树形字段(full_path/is_leaf/level/parent_code)对齐 cmx_permission 命名约定。
use modql::field::Fields;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

/// 菜单实体（完整字段，用于查询返回）
#[derive(Debug, Clone, Serialize, Deserialize, Fields, FromRow)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct Menu {
    pub id: String,
    /// 菜单编码
    pub code: String,
    /// 菜单名称
    pub name: String,
    /// 父菜单ID(逻辑关联，无外键约束)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    /// 父菜单编码(根为NULL)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_code: Option<String>,
    /// 菜单全路径，如 /gl:finance/gl:dashboard
    pub full_path: String,
    /// 是否叶子节点：1-是，0-否
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_leaf: Option<i32>,
    /// 层级深度，根=1
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<i32>,
    /// 菜单描述
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 前端路由路径
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// 菜单图标
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// 前端组件路径
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component: Option<String>,
    /// 排序序号
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<i32>,
    /// 是否可见：0-隐藏，1-显示
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visible: Option<i32>,
    /// 扩展字段(用户自定义JSON文本)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extension: Option<String>,
    /// 所属域编码
    pub domain_code: String,
    /// 所属应用编码
    pub application_code: String,
    /// 所属模块编码
    pub module_code: String,
    /// 状态（0: 禁用, 1: 启用）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<i32>,
    /// 是否归档（0: 否, 1: 是）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archived: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_name: Option<String>,
}

/// 创建 DTO（不含 full_path/is_leaf/level/parent_code，由 Service 自动计算）
#[derive(Debug, Clone, Serialize, Deserialize, Fields)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct MenuForCreate {
    /// 菜单编码
    pub code: String,
    /// 菜单名称
    pub name: String,
    /// 父菜单ID(可选，None 表示根节点)
    pub parent_id: Option<String>,
    /// 前端路由路径
    pub path: Option<String>,
    /// 菜单图标
    pub icon: Option<String>,
    /// 前端组件路径
    pub component: Option<String>,
    /// 排序序号
    pub sort_order: i32,
    /// 是否可见：0-隐藏，1-显示
    pub visible: i32,
    /// 扩展字段(用户自定义JSON文本)
    pub extension: Option<String>,
    /// 所属域编码
    pub domain_code: String,
    /// 所属应用编码
    pub application_code: String,
    /// 所属模块编码
    pub module_code: String,
}

/// 更新 DTO（所有字段可选）
#[derive(Debug, Clone, Serialize, Deserialize, Fields, Default)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct MenuForUpdate {
    pub name: Option<String>,
    pub path: Option<String>,
    pub icon: Option<String>,
    pub component: Option<String>,
    pub sort_order: Option<i32>,
    pub visible: Option<i32>,
    pub extension: Option<String>,
    pub status: Option<i32>,
}
