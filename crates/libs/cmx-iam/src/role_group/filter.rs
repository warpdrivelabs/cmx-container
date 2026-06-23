//! 角色组 Filter 定义

use modql::filter::{FilterNodes, OpValsInt64, OpValsString};
use serde::Deserialize;

/// 角色组查询过滤器。
///
/// 通过 modql `OpVals` 支持多种操作符（如 `Eq` / `In` / `Like` 等），
/// 用于 `page_role_groups` / `list_role_groups` 等查询接口。
#[derive(Debug, Clone, FilterNodes, Deserialize, Default)]
pub struct RoleGroupFilter {
    /// 按角色组名称过滤。
    pub name: Option<OpValsString>,

    /// 按父角色组 ID 过滤。
    pub parent_id: Option<OpValsString>,

    /// 按归档标记过滤（0 未归档 / 1 已归档），Service 层默认追加 `Eq(0)`。
    pub archived: Option<OpValsInt64>,
}
