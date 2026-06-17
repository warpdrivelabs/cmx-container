//! 权限 BMC 定义

use cmx_database::crud::DbBmc;

/// 权限表 BMC。
pub struct PermissionBmc;
impl DbBmc for PermissionBmc {
    const TABLE: &'static str = "cmx_permission";
    const PK_COLUMN: &'static str = "id";
}
