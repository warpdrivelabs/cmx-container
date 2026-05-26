//! 文档生成模块

pub mod ast_json_gen;
pub mod json_gen;

pub use ast_json_gen::{generate_ast_document, AstScanResult};
pub use json_gen::{generate_document, ScanResult};
