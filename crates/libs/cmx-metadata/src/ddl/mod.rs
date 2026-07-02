//! DDL 生成模块
//!
//! 提供 DDL 方言 trait 定义和便捷函数。
//! 每种数据库通过实现 `DdlDialect` trait 来产出其特定的 DDL 语法。
//!
//! # 功能特性
//! - 定义 `DdlDialect` trait，统一 DDL 生成接口
//! - 提供 PostgreSQL 方言实现
//! - 便捷函数支持快速生成单表或多表 DDL
//!
//! # 使用示例
//! ```ignore
//! use cmx_metadata::ddl::{DdlDialect, table_to_pg_ddl};
//! use cmx_metadata::ddl::postgres::PostgresDdlDialect;
//!
//! // 使用便捷函数
//! let ddl = table_to_pg_ddl(&table)?;
//!
//! // 或使用方言
//! let dialect = PostgresDdlDialect::default();
//! let ddl = dialect.generate_full_ddl(&table)?;
//! ```

pub mod diff;
pub mod postgres;

use crate::MetadataError;
use cmx_core::model::cell::{ColumnDefine, TableDefine};

/// DDL 方言 trait
///
/// 每种数据库实现此 trait 以产出其特定的 DDL 语法。
/// 包含创建表、索引、注释、修改表结构等操作的生成方法。
pub trait DdlDialect {
    /// 返回方言名称（如 "PostgreSQL"、"MySQL"）
    fn dialect_name(&self) -> &str;

    /// 将 FieldType + 列元数据映射为具体的 SQL 类型字符串
    fn map_column_type(&self, col: &ColumnDefine) -> String;

    /// 生成 CREATE TABLE 语句
    fn generate_create_table(&self, table: &TableDefine) -> Result<String, MetadataError>;

    /// 生成 CREATE INDEX 语句列表
    fn generate_create_indexes(&self, table: &TableDefine) -> Result<Vec<String>, MetadataError>;

    /// 生成 COMMENT ON 语句列表（表注释 + 列注释）
    fn generate_comments(&self, table: &TableDefine) -> Result<Vec<String>, MetadataError>;

    /// 生成完整 DDL（CREATE TABLE + INDEX + COMMENT）
    ///
    /// # 参数
    /// * `table` - 表定义
    ///
    /// # 返回值
    /// * 成功返回完整 DDL 字符串
    /// * 失败返回 `MetadataError`
    fn generate_full_ddl(&self, table: &TableDefine) -> Result<String, MetadataError> {
        let mut parts = Vec::new();
        parts.push(self.generate_create_table(table)?);
        for stmt in self.generate_comments(table)? {
            parts.push(stmt);
        }
        for stmt in self.generate_create_indexes(table)? {
            parts.push(stmt);
        }
        Ok(parts.join("\n\n"))
    }

    /// 生成多张表的完整 DDL
    ///
    /// # 参数
    /// * `tables` - 表定义列表
    ///
    /// # 返回值
    /// * 成功返回完整 DDL 字符串
    /// * 失败返回 `MetadataError`
    fn generate_full_ddl_for_tables(
        &self,
        tables: &[TableDefine],
    ) -> Result<String, MetadataError> {
        let mut all = Vec::new();
        for table in tables {
            all.push(self.generate_full_ddl(table)?);
        }
        Ok(all.join("\n\n"))
    }

    /// 生成 ALTER TABLE ADD COLUMN 语句
    fn generate_add_column(
        &self,
        table_name: &str,
        schema: Option<&str>,
        col: &ColumnDefine,
    ) -> Result<String, MetadataError>;

    /// 生成 ALTER TABLE DROP COLUMN 语句
    fn generate_drop_column(
        &self,
        table_name: &str,
        schema: Option<&str>,
        col_name: &str,
    ) -> Result<String, MetadataError>;

    /// 生成 ALTER TABLE ALTER COLUMN 语句（类型/nullable/default 变更）
    fn generate_alter_column(
        &self,
        table_name: &str,
        schema: Option<&str>,
        old_col: &ColumnDefine,
        new_col: &ColumnDefine,
    ) -> Result<Vec<String>, MetadataError>;

    /// 生成 DROP TABLE IF EXISTS 语句
    fn generate_drop_table(&self, table: &TableDefine) -> Result<String, MetadataError>;
}

// ==========================================
// 便捷函数
// ==========================================

/// 生成单表的 PostgreSQL DDL
///
/// # 参数
/// * `table` - 表定义
///
/// # 返回值
/// * 成功返回 DDL 字符串
/// * 失败返回 `MetadataError`
pub fn table_to_pg_ddl(table: &TableDefine) -> Result<String, MetadataError> {
    let dialect = postgres::PostgresDdlDialect::default();
    dialect.generate_full_ddl(table)
}

/// 生成多表的 PostgreSQL DDL
///
/// # 参数
/// * `tables` - 表定义列表
///
/// # 返回值
/// * 成功返回 DDL 字符串
/// * 失败返回 `MetadataError`
pub fn tables_to_pg_ddl(tables: &[TableDefine]) -> Result<String, MetadataError> {
    let dialect = postgres::PostgresDdlDialect::default();
    dialect.generate_full_ddl_for_tables(tables)
}

/// 生成 round-trip 模式的 PostgreSQL DDL（优先使用 col.db_type）
///
/// round-trip 模式下，会优先使用列定义中指定的 `db_type` 字段，
/// 而非根据 `FieldType` 自动映射。这用于保持与现有数据库的兼容性。
///
/// # 参数
/// * `table` - 表定义
///
/// # 返回值
/// * 成功返回 DDL 字符串
/// * 失败返回 `MetadataError`
pub fn table_to_pg_ddl_roundtrip(table: &TableDefine) -> Result<String, MetadataError> {
    let dialect = postgres::PostgresDdlDialect {
        prefer_db_type: true,
    };
    dialect.generate_full_ddl(table)
}
