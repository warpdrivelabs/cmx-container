//! 表定义加载模块
//!
//! 提供从 JSON 文件加载 `TableDefine` 的功能。
//! 支持三种 JSON 根格式：
//! - 单个 `TableDefine` 对象
//! - `{ "tables": [ ... ] }` 对象格式
//! - 顶层数组 `[ TableDefine, ... ]` 格式
//!
//! 本模块是从 cmx-core 迁移过来的。

use std::path::Path;

use serde::Deserialize;
use serde_json::Value;
use tracing::error;
use cmx_core::model::cell::TableDefine;
use crate::MetadataError;

/// 支持"多表"的 JSON 根结构（可选）
///
/// 支持三种格式：
/// - 单个 `TableDefine` 对象
/// - `{ "tables": [ ... ] }` 对象格式
/// - 顶层数组 `[ TableDefine, ... ]` 格式
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum TableDefinesRoot {
    /// 单个表定义
    // Single(Box<TableDefine>),
    /// 多表对象格式（包含 `tables` 键）
    Multi { tables: Vec<TableDefine> },
    /// 多表数组格式
    Array(Vec<TableDefine>),
}

/// 从 JSON 字符串解析单个表定义
///
/// # 参数
/// * `s` - JSON 字符串
///
/// # 返回值
/// * 成功返回 `TableDefine`
/// * 失败返回 `MetadataError`
pub fn table_define_from_str(s: &str) -> Result<TableDefine, MetadataError> {
    let define: TableDefine = serde_json::from_str(s)?;
    Ok(define)
}

/// 从 JSON 文件路径读取单个表定义
///
/// # 参数
/// * `path` - JSON 文件路径
///
/// # 返回值
/// * 成功返回 `TableDefine`
/// * 失败返回 `MetadataError`
pub fn load_table_define_from_path(path: &Path) -> Result<TableDefine, MetadataError> {
    let s = std::fs::read_to_string(path)?;
    table_define_from_str(&s)
}

/// 从 JSON 字符串解析多个表定义
///
/// 支持三种根格式：单个 `TableDefine` 对象、`{ "tables": [ ... ] }`、或顶层数组 `[ TableDefine, ... ]`
///
/// # 参数
/// * `s` - JSON 字符串
///
/// # 返回值
/// * 成功返回 `Vec<TableDefine>`
/// * 失败返回 `MetadataError`
pub fn table_defines_from_str(s: &str) -> Result<Vec<TableDefine>, MetadataError> {
    // let root: TableDefinesRoot = serde_json::from_str(s)?;
    // Ok(match root {
    //     // TableDefinesRoot::Single(t) => vec![*t],
    //     TableDefinesRoot::Multi { tables } => tables,
    //     TableDefinesRoot::Array(arr) => arr,
    // })

    let json_value: Value = serde_json::from_str(s)?;

    // 链式调用：取值 -> 转数组 -> 转结构体
    // 注意：这里需要处理 Result 的转换，稍微复杂一点点，但逻辑很顺
    json_value
        .get("tables")
        .ok_or_else(|| MetadataError::ConfigNotFound("缺少 tables 字段".to_string())) // 转为 Result
        .and_then(|v| serde_json::from_value(v.clone()).map_err(MetadataError::from)) // 执行转换

}

/// 从 JSON 文件路径读取多个表定义
///
/// # 参数
/// * `path` - JSON 文件路径
///
/// # 返回值
/// * 成功返回 `Vec<TableDefine>`
/// * 失败返回 `MetadataError`
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
