//! Domain 实体的 Filter 定义
//!
//! 使用 modql 定义查询过滤器

use modql::filter::{FilterNodes, OpValsString, OpValsInt64};
use serde::Deserialize;

/// Domain 查询过滤器
///
/// 支持按 code、name、type、status 等字段进行过滤
#[derive(Debug, Clone, FilterNodes, Deserialize, Default)]
pub struct DomainFilter {
    /// 按 code 过滤
    pub code: Option<OpValsString>,
    /// 按名称过滤
    pub name: Option<OpValsString>,
    /// 按类型过滤
    pub r#type: Option<OpValsString>,
    /// 按状态过滤
    pub status: Option<OpValsInt64>,
    /// 按归档状态过滤
    pub archived: Option<OpValsInt64>,
}
