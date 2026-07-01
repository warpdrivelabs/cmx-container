//! Form 实体的 Filter 定义
//!
//! 使用 modql 定义查询过滤器

use modql::filter::{FilterNodes, OpValsInt64, OpValsString};
use serde::Deserialize;

/// Form 查询过滤器
///
/// 支持按 code、name、domain_code、application_code、module_code、status 等字段过滤
#[derive(Debug, Clone, FilterNodes, Deserialize, Default)]
pub struct FormFilter {
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
    /// 状态过滤
    pub status: Option<OpValsInt64>,
    /// 归档标志过滤
    pub archived: Option<OpValsInt64>,
}
