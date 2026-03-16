//! DDL 生成模块 — DdlDialect trait 定义 + 便捷函数

pub mod postgres;
pub mod diff;

use cmx_core::model::cell::{ColumnDefine, TableDefine};
use crate::MetadataError;

/// DDL 方言 trait — 每种数据库实现此 trait 以产出其特定的 DDL 语法
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
    fn generate_full_ddl_for_tables(&self, tables: &[TableDefine]) -> Result<String, MetadataError> {
        let mut all = Vec::new();
        for table in tables {
            all.push(self.generate_full_ddl(table)?);
        }
        Ok(all.join("\n\n"))
    }

    /// 生成 ALTER TABLE ADD COLUMN 语句
    fn generate_add_column(&self, table_name: &str, schema: Option<&str>, col: &ColumnDefine) -> Result<String, MetadataError>;

    /// 生成 ALTER TABLE DROP COLUMN 语句
    fn generate_drop_column(&self, table_name: &str, schema: Option<&str>, col_name: &str) -> Result<String, MetadataError>;

    /// 生成 ALTER TABLE ALTER COLUMN 语句（类型/nullable/default 变更）
    fn generate_alter_column(&self, table_name: &str, schema: Option<&str>, old_col: &ColumnDefine, new_col: &ColumnDefine) -> Result<Vec<String>, MetadataError>;

    /// 生成 DROP TABLE IF EXISTS 语句
    fn generate_drop_table(&self, table: &TableDefine) -> Result<String, MetadataError>;
}

// ==========================================
// 便捷函数
// ==========================================

/// 生成单表的 PostgreSQL DDL
pub fn table_to_pg_ddl(table: &TableDefine) -> Result<String, MetadataError> {
    let dialect = postgres::PostgresDdlDialect::default();
    dialect.generate_full_ddl(table)
}

/// 生成多表的 PostgreSQL DDL
pub fn tables_to_pg_ddl(tables: &[TableDefine]) -> Result<String, MetadataError> {
    let dialect = postgres::PostgresDdlDialect::default();
    dialect.generate_full_ddl_for_tables(tables)
}

/// 生成 round-trip 模式的 PostgreSQL DDL（优先使用 col.db_type）
pub fn table_to_pg_ddl_roundtrip(table: &TableDefine) -> Result<String, MetadataError> {
    let dialect = postgres::PostgresDdlDialect { prefer_db_type: true };
    dialect.generate_full_ddl(table)
}
