//! cmx-metadata — 表定义元数据管理
//!
//! 职责：JSON 配置加载、DDL 生成/解析、增量 DDL diff、DDL 执行、i18n 伴生表生成。
//! 基础结构体（TableDefine、ColumnDefine 等）定义在 cmx-core 中。
//! DDL 执行依赖 cmx-database 提供的底层 SQL 执行能力。

pub mod config;
pub mod ddl;
pub mod error;
pub mod executor;
pub mod i18n;
pub mod loader;
pub mod parser;
pub mod seed;

pub use ddl::DdlDialect;
pub use ddl::diff::{ColumnChange, ColumnCommentChange, DdlDiff, IndexChange, TableChange};
pub use ddl::postgres::PostgresDdlDialect;
pub use error::MetadataError;
pub use executor::{
    BaseError, PgTableDefineExecutor, TableDefineDbExecutor, execute_ddl_by_ids,
    execute_ddl_statement_by_ids,
};
