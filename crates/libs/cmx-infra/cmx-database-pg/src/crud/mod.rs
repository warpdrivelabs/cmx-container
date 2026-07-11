// region:    --- 模块

mod crud_fns;
mod custom_query;
mod error;
// mod macro_utils;
mod count_optimizer;
mod utils;

// -- 展平用户代码的层级结构。
pub use count_optimizer::{CountOptimizerConfig, generate_count_sql};
pub use crud_fns::*;
pub use custom_query::*;
pub use error::*;
pub use utils::*;

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
        TableRef::Table(SIden(Self::TABLE).into_iden().into(), None)
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

    /// 声明需要加密存储的字段名列表
    /// 返回字段名数组，如 &["db_url", "password"]
    /// 默认返回空数组，表示无加密字段（向后兼容）
    fn encrypted_fields() -> &'static [&'static str] {
        &[]
    }

    /// 指定此 Bmc 的表是否启用 `archived` 过滤。
    /// 为 true 时，GenericCrudService::get 自动追加 `archived = 0` 过滤，
    /// 确保已归档（逻辑删除）的数据不会被查出。
    /// 物理删除的表（无 archived=1 残留行）应覆写为 false。
    ///
    /// 默认值：true
    fn use_archived() -> bool {
        true
    }
}
