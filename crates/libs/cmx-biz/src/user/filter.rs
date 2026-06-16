//! User/Role/Permission Filter 定义

use modql::filter::{FilterNodes, OpValsInt64, OpValsString};
use serde::Deserialize;

/// 用户过滤器
#[derive(Debug, Clone, FilterNodes, Deserialize, Default)]
pub struct UserFilter {
    pub username: Option<OpValsString>,
    pub nickname: Option<OpValsString>,
    pub status: Option<OpValsInt64>,
    pub archived: Option<OpValsInt64>,
}

/// 角色过滤器
#[derive(Debug, Clone, FilterNodes, Deserialize, Default)]
pub struct RoleFilter {
    pub code: Option<OpValsString>,
    pub name: Option<OpValsString>,
    pub status: Option<OpValsInt64>,
    pub archived: Option<OpValsInt64>,
}

/// 权限过滤器
#[derive(Debug, Clone, FilterNodes, Deserialize, Default)]
pub struct PermissionFilter {
    pub code: Option<OpValsString>,
    pub name: Option<OpValsString>,
    pub r#type: Option<OpValsString>,
    pub status: Option<OpValsInt64>,
}
