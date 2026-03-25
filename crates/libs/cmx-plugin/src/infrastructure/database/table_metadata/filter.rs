//! 表元数据查询过滤器
//!
//! 使用 modql 实现过滤器

use modql::filter::{FilterNodes, OpValsInt64, OpValsString};
use serde::Deserialize;

/// 表元数据查询过滤器
///
/// 使用 modql 实现，支持多种操作符
#[derive(Debug, Clone, FilterNodes, Deserialize, Default)]
pub struct TableMetadataFilter {
    /// 按表名过滤（支持模糊查询：Contains, StartsWith, EndsWith）
    pub table_name: Option<OpValsString>,
    /// 按数据库ID过滤
    pub db_id: Option<OpValsString>,
    /// 按插件ID过滤
    pub plugin_id: Option<OpValsString>,
    /// 按域编码过滤
    pub domain_code: Option<OpValsString>,
    /// 按应用编码过滤
    pub application_code: Option<OpValsString>,
    /// 按模块编码过滤
    pub module_code: Option<OpValsString>,
    /// 按归档状态过滤
    pub archived: Option<OpValsInt64>,
}

/// 表元数据版本查询过滤器
#[derive(Debug, Clone, FilterNodes, Deserialize, Default)]
pub struct TableMetadataVersionFilter {
    /// 按表名过滤
    pub table_name: Option<OpValsString>,
    /// 按数据库ID过滤
    pub db_id: Option<OpValsString>,
    /// 按插件ID过滤
    pub plugin_id: Option<OpValsString>,
    /// 按版本过滤
    pub version: Option<OpValsString>,
}
