//! SysDatasource 实体的 Filter 定义
//!
//! 使用 modql 定义查询过滤器

use modql::filter::{FilterNodes, OpValsString, OpValsInt64};
use serde::Deserialize;

/// SysDatasource 查询过滤器
///
/// 支持按 id、db_id、db_type、域应用模块、source_type、status 等字段进行过滤
#[derive(Debug, Clone, FilterNodes, Deserialize, Default)]
pub struct SysDatasourceFilter {
    /// 主键过滤
    pub id: Option<OpValsString>,
    /// 数据源标识过滤
    pub db_id: Option<OpValsString>,
    /// 数据库类型过滤
    pub db_type: Option<OpValsString>,
    /// 默认标志过滤
    pub default_flag: Option<OpValsInt64>,
    /// 数据源来源过滤
    pub source: Option<OpValsString>,
    /// 所属域编码过滤
    pub domain_code: Option<OpValsString>,
    /// 所属应用编码过滤
    pub application_code: Option<OpValsString>,
    /// 所属模块编码过滤
    pub module_code: Option<OpValsString>,
    /// 数据源类型过滤
    pub source_type: Option<OpValsString>,
    /// 状态过滤
    pub status: Option<OpValsInt64>,
    /// 归档标志过滤
    pub archived: Option<OpValsInt64>,
}
