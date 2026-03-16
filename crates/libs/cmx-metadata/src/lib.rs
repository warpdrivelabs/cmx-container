//! cmx-metadata — 表定义元数据管理
//!
//! 职责：JSON 配置加载、DDL 生成/解析、增量 DDL diff、i18n 伴生表生成。
//! 基础结构体（TableDefine、ColumnDefine 等）定义在 cmx-core 中。

pub mod error;
pub mod loader;
pub mod config;
pub mod i18n;
pub mod ddl;
pub mod parser;

pub use error::MetadataError;
