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
    #[modql(rel = "cmx_meta_table_define")]
    pub table_name: Option<OpValsString>,
    /// 按显示名过滤
    #[modql(rel = "cmx_meta_table_define")]
    pub display_name: Option<OpValsString>,
    /// 按数据库ID过滤
    #[modql(rel = "cmx_meta_table_define")]
    pub db_id: Option<OpValsString>,
    /// 按插件ID过滤
     #[modql(rel = "cmx_meta_table_define")]
    pub plugin_id: Option<OpValsString>,
    /// 按域编码过滤
    #[modql(rel = "cmx_meta_table_define")]
    pub domain_code: Option<OpValsString>,
    #[modql(rel = "cmx_meta_table_define")]
    pub application_code: Option<OpValsString>,
    /// 按模块编码过滤
    #[modql(rel = "cmx_meta_table_define")]
    pub module_code: Option<OpValsString>,
    /// 按归档状态过滤
    pub archived: Option<OpValsInt64>,
}

/// 表元数据版本查询过滤器
#[derive(Debug, Clone, FilterNodes, Deserialize, Default)]
pub struct TableMetadataVersionFilter {
    /// 按表名过滤
    #[modql(rel = "cmx_meta_table_define_version")]
    pub table_name: Option<OpValsString>,
    /// 按数据库ID过滤
    #[modql(rel = "cmx_meta_table_define_version")]
    pub db_id: Option<OpValsString>,
    /// 按插件ID过滤
    #[modql(rel = "cmx_meta_table_define_version")]
    pub plugin_id: Option<OpValsString>,
    /// 按版本过滤
    #[modql(rel = "cmx_meta_table_define_version")]
    pub version: Option<OpValsString>,
}
