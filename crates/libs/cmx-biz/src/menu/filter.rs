//! Menu 实体的 Filter 定义
//!
//! 使用 modql 定义查询过滤器(含树形字段过滤)

use modql::filter::{FilterNodes, OpValsInt64, OpValsString};
use serde::Deserialize;

/// Menu 查询过滤器
#[derive(Debug, Clone, FilterNodes, Deserialize, Default)]
pub struct MenuFilter {
    /// 编码过滤
    pub code: Option<OpValsString>,
    /// 名称过滤
    pub name: Option<OpValsString>,
    /// 父菜单ID过滤
    pub parent_id: Option<OpValsString>,
    /// 父菜单编码过滤
    pub parent_code: Option<OpValsString>,
    /// 全路径过滤(支持 $startsWith 前缀查子树)
    pub full_path: Option<OpValsString>,
    /// 是否叶子节点过滤
    pub is_leaf: Option<OpValsInt64>,
    /// 层级过滤
    pub level: Option<OpValsInt64>,
    /// 域编码过滤
    pub domain_code: Option<OpValsString>,
    /// 应用编码过滤
    pub application_code: Option<OpValsString>,
    /// 模块编码过滤
    pub module_code: Option<OpValsString>,
    /// 是否可见过滤
    pub visible: Option<OpValsInt64>,
    /// 状态过滤
    pub status: Option<OpValsInt64>,
    /// 归档标志过滤
    pub archived: Option<OpValsInt64>,
}
