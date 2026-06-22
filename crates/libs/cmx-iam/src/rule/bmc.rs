//! 权限规则 BMC 定义

use cmx_database::crud::DbBmc;

/// 权限规则表 BMC。
pub struct PermissionRuleBmc;
impl DbBmc for PermissionRuleBmc {
    const TABLE: &'static str = "cmx_permission_rule";
    const PK_COLUMN: &'static str = "id";
}

/// 规则权限项表 BMC。
pub struct PermissionRuleItemBmc;
impl DbBmc for PermissionRuleItemBmc {
    const TABLE: &'static str = "cmx_permission_rule_item";
    const PK_COLUMN: &'static str = "id";
}
