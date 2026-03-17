//! DbBmc trait 定义
//!
//! 用于定义表的元信息，如表名、主键列名、是否有时间戳字段等。

use modql::SIden;
use sea_query::{IntoIden, TableRef};

/// DbBmc trait 必须为实体的 Bmc 结构体实现。
/// 它指定了元信息，如表名、主键列名、
/// 表是否具有时间戳列等，
/// 并随着代码演进而提供更多功能。
pub trait DbBmc {
    /// 表名
    const TABLE: &'static str;
    /// 主键列名，默认为 "code"
    const PK_COLUMN: &'static str = "code";

    /// 获取表引用
    fn table_ref() -> TableRef {
        TableRef::Table(SIden(Self::TABLE).into_iden())
    }

    /// 指定此 Bmc 的表具有时间戳列。
    /// 这将允许代码根据需要更新这些列。
    ///
    /// 默认值：true
    fn has_timestamps() -> bool {
        true
    }

    /// 指定由此 BMC 管理的实体表
    /// 是否具有需要在创建时设置的 `owner_id` 列。
    ///
    /// 默认值：false
    fn has_owner_id() -> bool {
        false
    }
}
