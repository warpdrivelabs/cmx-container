//! 互斥规则 BMC 定义

use cmx_database::crud::DbBmc;

/// 互斥规则表 BMC
pub struct ExclusionRuleBmc;
impl DbBmc for ExclusionRuleBmc {
    const TABLE: &'static str = "cmx_exclusion_rule";
    const PK_COLUMN: &'static str = "id";
}

/// 互斥对象明细表 BMC
pub struct ExclusionRuleItemBmc;
impl DbBmc for ExclusionRuleItemBmc {
    const TABLE: &'static str = "cmx_exclusion_rule_item";
    const PK_COLUMN: &'static str = "id";
}
