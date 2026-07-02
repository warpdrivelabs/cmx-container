//! AST 解析器子模块
//!
//! 提供基于 AST 的结构体解析和类型展开功能。
//!
//! # 模块结构
//!
//! * `struct_parser` - 解析 Rust 结构体定义
//! * `type_resolver` - 类型注册和解析

pub mod struct_parser;
pub mod type_resolver;

pub use struct_parser::{FieldDefinition, StructDefinition, parse_structs};
pub use type_resolver::{
    ResolvedField, TypeRegistry, extract_container_element, is_container_type, is_primitive_type,
};
