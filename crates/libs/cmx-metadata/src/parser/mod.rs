//! DDL 解析模块 — DdlParser trait 定义 + 便捷函数

pub mod postgres;

use cmx_core::model::cell::TableDefine;
use crate::MetadataError;

/// DDL 解析 trait — 每种数据库实现此 trait 以解析其 DDL 还原为 TableDefine
pub trait DdlParser {
    fn dialect_name(&self) -> &str;

    /// 解析单条 CREATE TABLE 语句，还原为 TableDefine
    fn parse_create_table(&self, ddl: &str) -> Result<TableDefine, MetadataError>;

    /// 解析完整 DDL 文本（可含多条 CREATE TABLE / CREATE INDEX / COMMENT），还原为多个 TableDefine
    fn parse_ddl(&self, ddl: &str) -> Result<Vec<TableDefine>, MetadataError>;
}

/// 解析 PostgreSQL DDL 还原为多个 TableDefine
pub fn pg_ddl_to_table_defines(ddl: &str) -> Result<Vec<TableDefine>, MetadataError> {
    let parser = postgres::PostgresDdlParser;
    parser.parse_ddl(ddl)
}

/// 解析 PostgreSQL DDL 还原为单个 TableDefine（取第一个）
pub fn pg_ddl_to_table_define(ddl: &str) -> Result<TableDefine, MetadataError> {
    let tables = pg_ddl_to_table_defines(ddl)?;
    tables
        .into_iter()
        .next()
        .ok_or_else(|| MetadataError::DdlParse("未找到 CREATE TABLE 语句".to_string()))
}
