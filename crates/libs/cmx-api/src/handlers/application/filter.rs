//! Application 实体的 Filter 定义
//!
//! 使用 modql 定义查询过滤器

use modql::filter::{FilterNodes, OpValsString, OpValsInt64};
use serde::Deserialize;

/// Application 查询过滤器
///
/// 支持按 code、name、domain_code、type、status 等字段进行过滤
#[derive(Debug, Clone, FilterNodes, Deserialize, Default)]
pub struct ApplicationFilter {
    /// 编码过滤
    pub code: Option<OpValsString>,
    /// 名称过滤
    pub name: Option<OpValsString>,
    /// 域编码过滤
    pub domain_code: Option<OpValsString>,
    /// 类型过滤
    pub r#type: Option<OpValsString>,
    /// 状态过滤
    pub status: Option<OpValsInt64>,
    /// 归档标志过滤
    pub archived: Option<OpValsInt64>,
}
