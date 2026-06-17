//! 权限 Filter 定义

use modql::filter::{FilterNodes, OpValsInt64, OpValsString};
use serde::Deserialize;

/// 权限查询过滤器。
///
/// 通过 modql `OpVals` 支持多种操作符（如 `Eq` / `In` / `Like` 等），
/// 用于 `page_permissions` / `list_permissions` 等查询接口。
#[derive(Debug, Clone, FilterNodes, Deserialize, Default)]
pub struct PermissionFilter {
    /// 按权限编码过滤。
    pub code: Option<OpValsString>,

    /// 按权限名称过滤。
    pub name: Option<OpValsString>,

    /// 按资源类型过滤（如 `menu` / `button` / `api`）。
    pub resource_type: Option<OpValsString>,

    /// 按父权限 ID 过滤。
    pub parent_id: Option<OpValsString>,

    /// 按状态过滤（1 启用 / 0 禁用）。
    pub status: Option<OpValsInt64>,

    /// 按归档标记过滤（0 未归档 / 1 已归档），Service 层默认追加 `Eq(0)`。
    pub archived: Option<OpValsInt64>,
}
