//! 模块资源定义契约（导入/导出对称结构体）。
//!
//! 这些结构体是模块包(forms/menus/metadata/permissions)的序列化契约,
//! 同时被 cmx-plugin(模块导入/导出)、cmx-biz(本地实现)、cmx-iam(权限)使用。
//! 与入库的业务实体(如 FormForCreate)解耦,仅承载「定义透传」语义。
//!
//! 已有的 `PermissionDefinition` 定义在 `crate::model::iam::permission`,
//! 本模块补充 Form / Menu 的契约结构体(Table 复用 `crate::model::meta::table::TableDefine`)。

use serde::{Deserialize, Serialize};

/// 表单定义（模块包 forms/*.json 的单条契约）。
///
/// 导入时整体 `definition` JSON 透传存入 `cmx_form.definition`;
/// 导出时从 `cmx_form` 查询组装。与 `FormForCreate` 对齐但仅含定义字段。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormDefinition {
    /// 表单编码（模块导入规则:`{module_code}:{file_stem}`）
    pub code: String,
    /// 表单名称
    pub name: String,
    /// 表单描述
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 表单完整定义 JSON（字段/布局/校验，整体透传）
    pub definition: serde_json::Value,
    /// 所属域编码
    pub domain_code: String,
    /// 所属应用编码
    pub application_code: String,
    /// 所属模块编码
    pub module_code: String,
}

/// 菜单节点定义（模块包 menus/*.json 的单个节点，一节点一行）。
///
/// 导入导出均使用**树状 JSON**:导出时模块下全部节点按 `parent_code` 组装成树(根节点数组,
/// 每个根含 `children`);导入时递归遍历树天然保证父先于子建立。
///
/// 存储模型为「一节点一行」(`cmx_menu`),树形衍生字段(`id`/`id_path`/`code_path`/`leaf`/
/// `depth`)由 Service 自动生成,不在契约中。已知业务字段均为一等字段;
/// `definition` 承载节点自身的额外自定义数据(整体透传),`children` 仅用于树状序列化。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MenuDefinition {
    /// 菜单编码（业务唯一标识，导入时直接使用，不重新生成）
    pub code: String,
    /// 菜单名称
    pub name: String,
    /// 父节点编码（根节点为 None；树状导入时由 children 嵌套关系隐含，亦可显式声明）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_code: Option<String>,
    /// 菜单描述
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 前端路由路径
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// 菜单图标
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// 前端组件路径
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component: Option<String>,
    /// 排序序号
    #[serde(default)]
    pub sort_order: i32,
    /// 是否可见：0-隐藏，1-显示
    #[serde(default = "default_visible")]
    pub visible: i32,
    /// 打开方式：0-应用页标签,1-浏览器标签,2-弹窗,3-抽屉,4-全屏显示,5-下拉菜单
    #[serde(default)]
    pub open_type: i32,
    /// 功能码，关联 `cmx_permission.code`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fun_code: Option<String>,
    /// 该节点自身的额外自定义数据（承载前端/插件的扩展字段，整体透传）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub definition: Option<serde_json::Value>,
    /// 扩展属性，存储 JSON 格式的额外业务属性
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ext_attributes: Option<String>,
    /// 子节点（仅用于树状 JSON 序列化/反序列化；DB 层一节点一行，不存储 children）
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<MenuDefinition>,
    /// 所属域编码
    pub domain_code: String,
    /// 所属应用编码
    pub application_code: String,
    /// 所属模块编码
    pub module_code: String,
}

/// `visible` 字段的反序列化默认值（1-显示）。
fn default_visible() -> i32 {
    1
}
