//! Form 实体的 DbBmc 实现
//!
//! 定义 cmx_form 表的元信息

use cmx_database::crud::DbBmc;

/// Form 实体的 Bmc
///
/// 定义了 cmx_form 表的元信息
pub struct FormBmc;

impl DbBmc for FormBmc {
    /// 表名
    const TABLE: &'static str = "cmx_form";

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

    /// 物理删除表，不启用 archived 过滤
    fn use_archived() -> bool {
        false
    }
}
