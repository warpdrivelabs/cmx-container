//! User/Role/Permission Bmc 定义

use cmx_database::crud::DbBmc;

/// 用户表 Bmc
pub struct UserBmc;
impl DbBmc for UserBmc {
    const TABLE: &'static str = "cmx_user";
    const PK_COLUMN: &'static str = "id";
}

/// 角色表 Bmc
pub struct RoleBmc;
impl DbBmc for RoleBmc {
    const TABLE: &'static str = "cmx_role";
    const PK_COLUMN: &'static str = "id";
}

/// 用户角色关联表 Bmc
pub struct UserRoleBmc;
impl DbBmc for UserRoleBmc {
    const TABLE: &'static str = "cmx_user_role";
    const PK_COLUMN: &'static str = "id";
}

/// 权限表 Bmc
pub struct PermissionBmc;
impl DbBmc for PermissionBmc {
    const TABLE: &'static str = "cmx_permission";
    const PK_COLUMN: &'static str = "id";
}

/// 角色权限关联表 Bmc
pub struct RolePermissionBmc;
impl DbBmc for RolePermissionBmc {
    const TABLE: &'static str = "cmx_role_permission";
    const PK_COLUMN: &'static str = "id";
}
