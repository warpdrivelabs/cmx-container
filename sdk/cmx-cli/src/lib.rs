//! CMX Plugin Documentation Generator
//!
//! 用于扫描 Rust 代码，识别 `#[plugin_fn]` 属性函数，
//! 解析文档注释并生成 JSON 格式的 API 文档。

pub mod ast_parser;
pub mod cli;
pub mod generator;
pub mod models;
pub mod parser;

pub use ast_parser::{ResolvedField, TypeRegistry, parse_structs};
pub use cli::commands::run;
pub use generator::ast_json_gen::generate_ast_document;
pub use models::doc_types::{FunctionDoc, PluginDocument};
pub use parser::ast_parser::parse_rust_file;
pub use parser::doc_parser::parse_doc_comments;
