//! DDL 解析模块
//!
//! 提供 DDL 解析 trait 定义和便捷函数。
//! 每种数据库通过实现 `DdlParser` trait 来解析其 DDL 还原为 TableDefine。
//!
//! # 功能特性
//! - 定义 `DdlParser` trait，统一 DDL 解析接口
//! - 提供 PostgreSQL 方言解析实现
//! - 便捷函数支持快速解析 DDL
//!
//! # 使用示例
//! ```ignore
//! use cmx_metadata::parser::{pg_ddl_to_table_defines, pg_ddl_to_table_define};
//!
//! let tables = pg_ddl_to_table_defines(ddl)?;
//! let table = pg_ddl_to_table_define(ddl)?;
//! ```

pub mod postgres;

use cmx_core::model::cell::TableDefine;
use crate::MetadataError;

/// DDL 解析 trait
///
/// 每种数据库实现此 trait 以解析其 DDL 还原为 TableDefine。
pub trait DdlParser {
    /// 返回方言名称（如 "PostgreSQL"、"MySQL"）
    fn dialect_name(&self) -> &str;

    /// 解析单条 CREATE TABLE 语句，还原为 TableDefine
    ///
    /// # 参数
    /// * `ddl` - CREATE TABLE 语句
    ///
    /// # 返回值
    /// * 成功返回 `TableDefine`
    /// * 失败返回 `MetadataError`
    fn parse_create_table(&self, ddl: &str) -> Result<TableDefine, MetadataError>;

    /// 解析完整 DDL 文本（可含多条 CREATE TABLE / CREATE INDEX / COMMENT），还原为多个 TableDefine
    ///
    /// # 参数
    /// * `ddl` - DDL 语句文本
    ///
    /// # 返回值
    /// * 成功返回 `Vec<TableDefine>`
    /// * 失败返回 `MetadataError`
    fn parse_ddl(&self, ddl: &str) -> Result<Vec<TableDefine>, MetadataError>;
}

/// 解析 PostgreSQL DDL 还原为多个 TableDefine
///
/// # 参数
/// * `ddl` - DDL 语句文本
///
/// # 返回值
/// * 成功返回 `Vec<TableDefine>`
/// * 失败返回 `MetadataError`
pub fn pg_ddl_to_table_defines(ddl: &str) -> Result<Vec<TableDefine>, MetadataError> {
    let parser = postgres::PostgresDdlParser;
    parser.parse_ddl(ddl)
}

/// 解析 PostgreSQL DDL 还原为单个 TableDefine（取第一个）
///
/// # 参数
/// * `ddl` - DDL 语句文本
///
/// # 返回值
/// * 成功返回 `TableDefine`
/// * 失败返回 `MetadataError`
pub fn pg_ddl_to_table_define(ddl: &str) -> Result<TableDefine, MetadataError> {
    let tables = pg_ddl_to_table_defines(ddl)?;
    tables
        .into_iter()
        .next()
        .ok_or_else(|| MetadataError::DdlParse("未找到 CREATE TABLE 语句".to_string()))
}
