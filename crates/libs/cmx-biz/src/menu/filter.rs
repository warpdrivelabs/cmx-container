//! Menu 实体的 Filter 定义
//!
//! 使用 modql 定义查询过滤器(含标准分级字段过滤)

use modql::filter::{FilterNodes, OpValsInt64, OpValsString};
use serde::Deserialize;

/// Menu 查询过滤器
#[derive(Debug, Clone, FilterNodes, Deserialize, Default)]
pub struct MenuFilter {
    /// 编码过滤
    pub code: Option<OpValsString>,
    /// 名称过滤
    pub name: Option<OpValsString>,
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
    // 标准分级字段过滤
    /// 父节点ID过滤
    pub parent_id: Option<OpValsString>,
    /// 父节点编码过滤
    pub parent_code: Option<OpValsString>,
    /// ID全路径过滤(支持 $startsWith 前缀查子树)
    pub id_path: Option<OpValsString>,
    /// 编号全路径过滤(支持 $startsWith 前缀查子树)
    pub code_path: Option<OpValsString>,
    /// 是否叶子节点过滤
    pub leaf: Option<OpValsInt64>,
    /// 级数过滤
    pub depth: Option<OpValsInt64>,
}
