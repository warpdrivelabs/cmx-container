//! 角色 BMC 定义

use cmx_database::crud::DbBmc;

/// 角色表 BMC
pub struct RoleBmc;
impl DbBmc for RoleBmc {
    const TABLE: &'static str = "cmx_role";
    const PK_COLUMN: &'static str = "id";
}

/// 角色权限关联表 BMC
pub struct RolePermissionBmc;
impl DbBmc for RolePermissionBmc {
    const TABLE: &'static str = "cmx_role_permission";
    const PK_COLUMN: &'static str = "id";
}
