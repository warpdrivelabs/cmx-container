//! 用户 BMC 定义

use cmx_database::crud::DbBmc;

/// 用户表 BMC。
pub struct UserBmc;
impl DbBmc for UserBmc {
    const TABLE: &'static str = "cmx_user";
    const PK_COLUMN: &'static str = "id";
}

/// 用户-角色关联表 BMC。
pub struct UserRoleBmc;
impl DbBmc for UserRoleBmc {
    const TABLE: &'static str = "cmx_user_role";
    const PK_COLUMN: &'static str = "id";
}
