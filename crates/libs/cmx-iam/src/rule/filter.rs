//! 互斥规则 Filter 定义。

use modql::filter::{FilterNodes, OpValsInt64, OpValsString};
use serde::Deserialize;

/// 互斥规则查询过滤器。
///
/// 通过 modql `OpVals` 支持多种操作符（如 `Eq` / `In` / `Contains` 等）。
///
/// - `code` / `name`：modql 模糊/精确匹配（前端可用 `$contains` 做模糊搜索）。
/// - `subject_type`：`permission` / `role`，精确匹配。
/// - `status`：1 启用 / 0 停用。
/// - `archived`：归档标记，Service 层默认追加 `Eq(0)`。
/// - `subject_id`：跨表过滤——匹配主主体（`primary_subject_id`）或任一排除项
///   （`cmx_exclusion_rule_item.subject_id`）。modql 标准路径仅支持单表 WHERE，
///   Service 层检测到该字段后走 raw SQL 分支；在 GenericCrudService 路径中
///   会被显式置 `None`（避免生成无效的 `exclusion_rule.subject_id` 列谓词）。
#[derive(Debug, Clone, FilterNodes, Deserialize, Default)]
pub struct ExclusionRuleFilter {
    /// 按规则编码过滤。
    pub code: Option<OpValsString>,

    /// 按规则名称过滤。
    pub name: Option<OpValsString>,

    /// 按主体类型过滤（permission / role）。
    pub subject_type: Option<OpValsString>,

    /// 按状态过滤（1 启用 / 0 停用）。
    pub status: Option<OpValsInt64>,

    /// 按归档标记过滤（0 未归档 / 1 已归档），Service 层默认追加 `Eq(0)`。
    pub archived: Option<OpValsInt64>,

    /// 跨表：关联的主体 ID（作为主主体或排除项）。
    ///
    /// 该字段由 Service 层检测并走 raw SQL 分支，不参与 modql 自动 WHERE
    /// （否则会生成无效的 `exclusion_rule.subject_id` 列谓词）。Service 在调用
    /// `GenericCrudService` 前会将其置 `None`。
    pub subject_id: Option<OpValsString>,
}
