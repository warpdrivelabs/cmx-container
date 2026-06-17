//! 用户 Filter 定义

use modql::filter::{FilterNodes, OpValsInt64, OpValsString};
use serde::Deserialize;

/// 用户过滤器
#[derive(Debug, Clone, FilterNodes, Deserialize, Default)]
pub struct UserFilter {
    pub username: Option<OpValsString>,
    pub nickname: Option<OpValsString>,
    pub email: Option<OpValsString>,
    pub phone: Option<OpValsString>,
    pub org_id: Option<OpValsString>,
    pub status: Option<OpValsInt64>,
    pub archived: Option<OpValsInt64>,
}
