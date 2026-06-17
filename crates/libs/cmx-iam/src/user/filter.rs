//! 用户 Filter 定义

use modql::filter::{FilterNodes, OpValsInt64, OpValsString};
use serde::Deserialize;

/// 用户查询过滤器。
///
/// 通过 modql `OpVals` 支持多种操作符（如 `Eq` / `In` / `Like` 等），
/// 用于 `page_users` / `list_users` 等查询接口。
#[derive(Debug, Clone, FilterNodes, Deserialize, Default)]
pub struct UserFilter {
    /// 按登录用户名过滤。
    pub username: Option<OpValsString>,

    /// 按昵称过滤。
    pub nickname: Option<OpValsString>,

    /// 按邮箱过滤。
    pub email: Option<OpValsString>,

    /// 按手机号过滤。
    pub phone: Option<OpValsString>,

    /// 按所属组织 ID 过滤。
    pub org_id: Option<OpValsString>,

    /// 按账户状态过滤（如 1 启用 / 0 禁用）。
    pub status: Option<OpValsInt64>,

    /// 按归档标记过滤（0 未归档 / 1 已归档），Service 层默认追加 `Eq(0)`。
    pub archived: Option<OpValsInt64>,
}
