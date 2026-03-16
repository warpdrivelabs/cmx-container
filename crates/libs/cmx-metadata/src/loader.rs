//! 从 JSON 加载 TableDefine（从 cmx-core meta/base.rs 迁移）

use std::path::Path;

use serde::Deserialize;

use cmx_core::model::cell::TableDefine;
use crate::MetadataError;

/// 支持"多表"的 JSON 根结构（可选）
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum TableDefinesRoot {
    Single(Box<TableDefine>),
    Multi { tables: Vec<TableDefine> },
    Array(Vec<TableDefine>),
}

/// 从 JSON 字符串解析单个表定义
pub fn table_define_from_str(s: &str) -> Result<TableDefine, MetadataError> {
    let define: TableDefine = serde_json::from_str(s)?;
    Ok(define)
}

/// 从 JSON 文件路径读取单个表定义
pub fn load_table_define_from_path(path: &Path) -> Result<TableDefine, MetadataError> {
    let s = std::fs::read_to_string(path)?;
    table_define_from_str(&s)
}

/// 从 JSON 字符串解析多个表定义
/// 支持三种根格式：单个 `TableDefine` 对象、`{ "tables": [ ... ] }`、或顶层数组 `[ TableDefine, ... ]`
pub fn table_defines_from_str(s: &str) -> Result<Vec<TableDefine>, MetadataError> {
    let root: TableDefinesRoot = serde_json::from_str(s)?;
    Ok(match root {
        TableDefinesRoot::Single(t) => vec![*t],
        TableDefinesRoot::Multi { tables } => tables,
        TableDefinesRoot::Array(arr) => arr,
    })
}

/// 从 JSON 文件路径读取多个表定义
pub fn load_table_defines_from_path(path: &Path) -> Result<Vec<TableDefine>, MetadataError> {
    let s = std::fs::read_to_string(path)?;
    table_defines_from_str(&s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_single_table() {
        let json = r#"{
            "table_name": "test_table",
            "display_name": "测试表",
            "columns": [],
            "version": 1
        }"#;
        let table = table_define_from_str(json).unwrap();
        assert_eq!(table.table_name, "test_table");
        assert_eq!(table.display_name, "测试表");
    }

    #[test]
    fn test_parse_multi_tables_object() {
        let json = r#"{
            "tables": [
                { "table_name": "t1", "display_name": "表1", "columns": [] },
                { "table_name": "t2", "display_name": "表2", "columns": [] }
            ]
        }"#;
        let tables = table_defines_from_str(json).unwrap();
        assert_eq!(tables.len(), 2);
        assert_eq!(tables[0].table_name, "t1");
        assert_eq!(tables[1].table_name, "t2");
    }

    #[test]
    fn test_parse_multi_tables_array() {
        let json = r#"[
            { "table_name": "t1", "display_name": "表1", "columns": [] },
            { "table_name": "t2", "display_name": "表2", "columns": [] }
        ]"#;
        let tables = table_defines_from_str(json).unwrap();
        assert_eq!(tables.len(), 2);
    }
}
