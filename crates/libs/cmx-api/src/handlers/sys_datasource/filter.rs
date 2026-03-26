//! SysDatasource 实体的 Filter 定义
//!
//! 使用 modql 定义查询过滤器

use modql::filter::{FilterNodes, OpValsString, OpValsInt64};
use serde::Deserialize;

/// SysDatasource 查询过滤器
///
/// 支持按 id、db_id、db_type、status 等字段进行过滤
#[derive(Debug, Clone, FilterNodes, Deserialize, Default)]
pub struct SysDatasourceFilter {
    /// 按 id 过滤
    pub id: Option<OpValsString>,
    /// 按数据源标识过滤
    pub db_id: Option<OpValsString>,
    /// 按数据库类型过滤
    pub db_type: Option<OpValsString>,
    /// 按是否默认过滤
    pub default_flag: Option<OpValsInt64>,
    /// 按状态过滤
    pub status: Option<OpValsInt64>,
    /// 按归档状态过滤
    pub archived: Option<OpValsInt64>,
}
