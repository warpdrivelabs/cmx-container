//! Domain 实体的 DbBmc 实现
//!
//! 定义表名、主键等元信息

use cmx_database::crud::DbBmc;

/// Domain 实体的 Bmc
///
/// 定义了 cmx_domain 表的元信息
pub struct DomainBmc;

impl DbBmc for DomainBmc {
    /// 表名
    const TABLE: &'static str = "cmx_domain";
    
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
