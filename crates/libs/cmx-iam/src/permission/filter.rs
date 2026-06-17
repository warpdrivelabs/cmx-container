//! 权限 Filter 定义

use modql::filter::{FilterNodes, OpValsInt64, OpValsString};
use serde::Deserialize;

/// 权限过滤器
#[derive(Debug, Clone, FilterNodes, Deserialize, Default)]
pub struct PermissionFilter {
    pub code: Option<OpValsString>,
    pub name: Option<OpValsString>,
    pub resource_type: Option<OpValsString>,
    pub parent_id: Option<OpValsString>,
    pub status: Option<OpValsInt64>,
    pub archived: Option<OpValsInt64>,
}
