//! 表元数据的 DbBmc 定义
//!
//! 定义 cmx_meta_table_define 和 cmx_meta_table_define_version 表的元信息

use cmx_database::crud::DbBmc;

/// cmx_meta_table_define 表的 Bmc
pub struct TableMetadataBmc;

impl DbBmc for TableMetadataBmc {
    const TABLE: &'static str = "cmx_meta_table_define";
    const PK_COLUMN: &'static str = "id";

    fn has_timestamps() -> bool {
        true
    }

    fn has_owner_id() -> bool {
        false
    }
}

/// cmx_meta_table_define_version 表的 Bmc
pub struct TableMetadataVersionBmc;

impl DbBmc for TableMetadataVersionBmc {
    const TABLE: &'static str = "cmx_meta_table_define_version";
    const PK_COLUMN: &'static str = "id";

    fn has_timestamps() -> bool {
        true
    }

    fn has_owner_id() -> bool {
        false
    }
}
