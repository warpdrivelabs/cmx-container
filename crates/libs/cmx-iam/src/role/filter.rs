//! 角色 Filter 定义

use modql::filter::{FilterNodes, OpValsInt64, OpValsString};
use serde::Deserialize;

/// 角色查询过滤器。
///
/// 通过 modql `OpVals` 支持多种操作符（如 `Eq` / `In` / `Like` 等），
/// 用于 `page_roles` / `list_roles` 等查询接口。
#[derive(Debug, Clone, FilterNodes, Deserialize, Default)]
pub struct RoleFilter {
    /// 按角色编码过滤。
    pub code: Option<OpValsString>,

    /// 按角色名称过滤。
    pub name: Option<OpValsString>,

    /// 按数据权限范围过滤。
    pub data_scope: Option<OpValsInt64>,

    /// 按排序号过滤。
    pub sort_order: Option<OpValsInt64>,

    /// 按状态过滤（1 启用 / 0 禁用）。
    pub status: Option<OpValsInt64>,

    /// 按归档标记过滤（0 未归档 / 1 已归档），Service 层默认追加 `Eq(0)`。
    pub archived: Option<OpValsInt64>,
}
