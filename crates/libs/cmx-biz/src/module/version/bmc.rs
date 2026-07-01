//! 模块版本管理的 DbBmc 实现
//!
//! - `ModuleCurrentVersionBmc` → cmx_module_current_version(当前态,每模块一行)
//! - `ModuleVersionHistoryBmc` → cmx_module_version_history(历史,多行)

use cmx_database::crud::DbBmc;

/// 模块当前版本 Bmc
pub struct ModuleCurrentVersionBmc;

impl DbBmc for ModuleCurrentVersionBmc {
    const TABLE: &'static str = "cmx_module_current_version";
    const PK_COLUMN: &'static str = "id";
    fn has_timestamps() -> bool {
        true
    }
    fn has_owner_id() -> bool {
        false
    }
}

/// 模块版本历史 Bmc
pub struct ModuleVersionHistoryBmc;

impl DbBmc for ModuleVersionHistoryBmc {
    const TABLE: &'static str = "cmx_module_version_history";
    const PK_COLUMN: &'static str = "id";
    fn has_timestamps() -> bool {
        true
    }
    fn has_owner_id() -> bool {
        false
    }
}
