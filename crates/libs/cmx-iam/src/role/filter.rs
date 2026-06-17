//! 角色 Filter 定义

use modql::filter::{FilterNodes, OpValsInt64, OpValsString};
use serde::Deserialize;

/// 角色过滤器
#[derive(Debug, Clone, FilterNodes, Deserialize, Default)]
pub struct RoleFilter {
    pub code: Option<OpValsString>,
    pub name: Option<OpValsString>,
    pub data_scope: Option<OpValsInt64>,
    pub sort_order: Option<OpValsInt64>,
    pub status: Option<OpValsInt64>,
    pub archived: Option<OpValsInt64>,
}
