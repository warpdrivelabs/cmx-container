//! Menu 实体定义
//!
//! 定义 Menu 实体的数据结构，包括完整实体和创建/更新 DTO。
//! 树形字段使用标准分级字段(leaf/depth/parent_id/parent_code/id_path/code_path)。
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
    /// 打开方式：0-应用页标签,1-浏览器标签,2-弹窗,3-抽屉,4-全屏显示,5-下拉菜单
    #[serde(skip_serializing_if = "Option::is_none")]
    pub open_type: Option<i32>,
    /// 功能码，关联 cmx_permission.code
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fun_code: Option<String>,
    /// 所属域编码
    pub domain_code: String,
    /// 所属应用编码
    pub application_code: String,
    /// 所属模块编码
    pub module_code: String,
    /// 菜单完整定义JSON(items/children树形结构，整体透传)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub definition: Option<serde_json::Value>,
    /// 状态（0: 禁用, 1: 启用）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<i32>,
    // 标准分级字段
    /// 是否明细：1-是叶子节点，0-非叶子节点
    #[serde(skip_serializing_if = "Option::is_none")]
    pub leaf: Option<i32>,
    /// 级数：根节点为1，逐层递增
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depth: Option<i32>,
    /// 父节点ID，根节点为空
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    /// 父节点编码，根节点为空
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_code: Option<String>,
    /// ID全路径，以/分隔
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id_path: Option<String>,
    /// 编号全路径，以/分隔
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code_path: Option<String>,
    // 审计字段
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
    /// 扩展属性，存储JSON格式的额外业务属性
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext_attributes: Option<String>,
}

/// 创建 DTO（不含分级字段 leaf/depth/parent_code/id_path/code_path，由 Service 自动计算）
#[derive(Debug, Clone, Serialize, Deserialize, Fields)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct MenuForCreate {
    /// 菜单编码
    pub code: String,
    /// 菜单名称
    pub name: String,
    /// 菜单描述
    pub description: Option<String>,
    /// 父菜单ID(可选，None 表示根节点)
    pub parent_id: Option<String>,
    /// 父菜单编码(可选，与 parent_id 二选一；同时提供时 parent_code 优先)
    pub parent_code: Option<String>,
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
    /// 打开方式：0-应用页标签,1-浏览器标签,2-弹窗,3-抽屉,4-全屏显示,5-下拉菜单
    pub open_type: i32,
    /// 功能码，关联 cmx_permission.code
    pub fun_code: Option<String>,
    /// 菜单完整定义JSON(模块导入时整体 items/children 树形 JSON 透传存入)
    pub definition: Option<serde_json::Value>,
    /// 扩展属性，存储JSON格式的额外业务属性
    pub ext_attributes: Option<String>,
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
    pub description: Option<String>,
    pub path: Option<String>,
    pub icon: Option<String>,
    pub component: Option<String>,
    pub sort_order: Option<i32>,
    pub visible: Option<i32>,
    /// 打开方式：0-应用页标签,1-浏览器标签,2-弹窗,3-抽屉,4-全屏显示,5-下拉菜单
    pub open_type: Option<i32>,
    /// 功能码，关联 cmx_permission.code
    pub fun_code: Option<String>,
    pub status: Option<i32>,
    /// 父菜单ID(传值时触发移动:级联重算该节点及后代路径/depth,并维护新旧父 leaf)
    pub parent_id: Option<String>,
    /// 父菜单编码(与 parent_id 二选一；同时提供时 parent_code 优先)
    pub parent_code: Option<String>,
    /// 扩展属性，存储JSON格式的额外业务属性
    pub ext_attributes: Option<String>,
}

/// 菜单树节点数据(用于 TreeNode::from_list 组装树形结构)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct MenuTreeNodeData {
    /// 菜单主键 ID（雪花算法生成，树的唯一标识）
    pub id: String,
    /// 父菜单 ID（根节点为 None）
    pub parent_id: Option<String>,
    /// 菜单编码(唯一业务标识)
    pub code: String,
    /// 父节点编码(根节点为 None)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_code: Option<String>,
    /// 菜单名称
    pub name: String,
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
    pub sort_order: i32,
    /// 是否可见：0-隐藏，1-显示
    pub visible: i32,
    /// 打开方式：0-应用页标签,1-浏览器标签,2-弹窗,3-抽屉,4-全屏显示,5-下拉菜单
    pub open_type: i32,
    /// 功能码，关联 cmx_permission.code
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fun_code: Option<String>,
    /// 级数：根节点为1
    pub depth: i32,
    /// 所属域编码
    pub domain_code: String,
    /// 所属应用编码
    pub application_code: String,
    /// 所属模块编码
    pub module_code: String,
    /// 菜单完整定义JSON
    #[serde(skip_serializing_if = "Option::is_none")]
    pub definition: Option<serde_json::Value>,
    /// 扩展属性
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext_attributes: Option<String>,
}

impl cmx_api_types::TreeNodeData for MenuTreeNodeData {
    fn node_id(&self) -> &str {
        &self.id
    }

    fn parent_id(&self) -> Option<&str> {
        self.parent_id.as_deref()
    }

    fn sort_key(&self) -> i32 {
        self.sort_order
    }
}
