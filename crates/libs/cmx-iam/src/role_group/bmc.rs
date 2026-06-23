//! 角色组 BMC 定义

use cmx_database::crud::DbBmc;

/// 角色组表 BMC。
pub struct RoleGroupBmc;
impl DbBmc for RoleGroupBmc {
    const TABLE: &'static str = "cmx_role_group";
    const PK_COLUMN: &'static str = "id";
}
