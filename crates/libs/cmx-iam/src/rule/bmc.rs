//! 互斥规则 BMC 定义。
//!
//! 为互斥规则主表与明细表提供 `DbBmc` trait 实现，
//! 供 `GenericCrudService` 进行标准 CRUD 操作。

use cmx_database::crud::DbBmc;

/// 互斥规则表 BMC。
///
/// 对应数据库 `cmx_exclusion_rule` 表，提供主键列与表名常量。
pub struct ExclusionRuleBmc;
impl DbBmc for ExclusionRuleBmc {
    const TABLE: &'static str = "cmx_exclusion_rule";
    const PK_COLUMN: &'static str = "id";
}

/// 互斥对象明细表 BMC。
///
/// 对应数据库 `cmx_exclusion_rule_item` 表，提供主键列与表名常量。
pub struct ExclusionRuleItemBmc;
impl DbBmc for ExclusionRuleItemBmc {
    const TABLE: &'static str = "cmx_exclusion_rule_item";
    const PK_COLUMN: &'static str = "id";

    /// 物理删除表，不启用 archived 过滤
    fn use_archived() -> bool {
        false
    }
}
