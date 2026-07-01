//! Menu 实体的 DbBmc 实现
//!
//! 定义 cmx_menu 表的元信息

use cmx_database::crud::DbBmc;

/// Menu 实体的 Bmc
pub struct MenuBmc;

impl DbBmc for MenuBmc {
    /// 表名
    const TABLE: &'static str = "cmx_menu";

    /// 主键列名
    const PK_COLUMN: &'static str = "id";

    /// 是否有时间戳字段
    fn has_timestamps() -> bool {
        true
    }

    /// 是否有 owner_id 字段
    fn has_owner_id() -> bool {
        false
    }
}
