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

/// 菜单定义（模块包 menus/*.json 的单条契约，根菜单）。
///
/// 导入时每个 definition 含完整菜单树(items/children),整体透传存入 `cmx_menu.definition`;
/// 导出时只导出根菜单(parent_id IS NULL),其 definition 含完整菜单树。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MenuDefinition {
    /// 菜单编码（模块导入规则:`{module_code}:{file_stem}`）
    pub code: String,
    /// 菜单名称
    pub name: String,
    /// 菜单完整定义 JSON（含 items/children 树形结构，整体透传）
    pub definition: serde_json::Value,
    /// 所属域编码
    pub domain_code: String,
    /// 所属应用编码
    pub application_code: String,
    /// 所属模块编码
    pub module_code: String,
}
