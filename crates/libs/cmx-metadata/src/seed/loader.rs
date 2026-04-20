//! 数据文件加载模块
//!
//! 支持从 JSON 和 CSV 文件加载种子数据，统一转换为 `Vec<serde_json::Value>` 格式。

use std::path::Path;

use cmx_core::model::cell::{ColumnDefine, FieldType};
use crate::MetadataError;

/// 从文件加载种子数据，自动根据扩展名选择 JSON 或 CSV 解析器
///
/// # 参数
/// * `path` - 数据文件路径
/// * `columns` - 目标表的列定义（CSV 模式用于类型转换）
///
/// # 返回值
/// * 成功返回数据行列表（每行为 serde_json::Value 对象）
/// * 失败返回 MetadataError
pub fn load_seed_data(
    path: &Path,
    columns: &[ColumnDefine],
) -> Result<Vec<serde_json::Value>, MetadataError> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "json" => load_seed_data_from_json(path),
        "csv" => load_seed_data_from_csv(path, columns),
        _ => Err(MetadataError::SeedData(format!(
            "不支持的种子数据文件格式: {:?}，仅支持 .json 和 .csv",
            ext
        ))),
    }
}

/// 从 JSON 文件加载种子数据
///
/// 支持顶层数组格式的 JSON：
/// [{ "col1": "value1", ... }, { "col1": "value2", ... }]
fn load_seed_data_from_json(path: &Path) -> Result<Vec<serde_json::Value>, MetadataError> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| MetadataError::SeedData(format!("读取 JSON 文件失败: {}", e)))?;

    let rows: Vec<serde_json::Value> = serde_json::from_str(&content)
        .map_err(|e| MetadataError::SeedData(format!("解析 JSON 文件失败: {}", e)))?;

    Ok(rows)
}

/// 从 CSV 文件加载种子数据
///
/// 首行为列名（表头），后续行为数据行。
/// 所有值根据 `TableDefine.columns` 的 `field_type` 进行类型转换。
fn load_seed_data_from_csv(
    path: &Path,
    columns: &[ColumnDefine],
) -> Result<Vec<serde_json::Value>, MetadataError> {
    use std::collections::HashMap;

    // 构建列名 → 类型的映射
    let col_type_map: HashMap<&str, &FieldType> = columns
        .iter()
        .map(|c| (c.name.as_str(), &c.field_type))
        .collect();

    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .from_path(path)
        .map_err(|e| MetadataError::SeedData(format!("打开 CSV 文件失败: {}", e)))?;

    let headers = reader
        .headers()
        .map_err(|e| MetadataError::SeedData(format!("读取 CSV 表头失败: {}", e)))?
        .iter()
        .map(|h| h.trim().to_string())
        .collect::<Vec<_>>();

    let mut rows = Vec::new();

    for (row_idx, result) in reader.records().enumerate() {
        let record = result.map_err(|e| {
            MetadataError::SeedData(format!("读取 CSV 第 {} 行失败: {}", row_idx + 2, e))
        })?;

        let mut row_map = serde_json::Map::new();

        for (col_idx, header) in headers.iter().enumerate() {
            let value_str = record.get(col_idx).unwrap_or("").trim();

            // 根据列定义的类型进行转换
            let json_value = if let Some(field_type) = col_type_map.get(header.as_str()) {
                convert_csv_value(value_str, field_type)
            } else {
                // 没有列定义时默认作为字符串
                serde_json::Value::String(value_str.to_string())
            };

            row_map.insert(header.clone(), json_value);
        }

        rows.push(serde_json::Value::Object(row_map));
    }

    Ok(rows)
}

/// 将 CSV 字符串值根据 FieldType 转换为 serde_json::Value
fn convert_csv_value(value: &str, field_type: &FieldType) -> serde_json::Value {
    if value.is_empty() {
        return serde_json::Value::Null;
    }

    match field_type {
        FieldType::Int => {
            if let Ok(v) = value.parse::<i64>() {
                serde_json::Value::Number(v.into())
            } else {
                serde_json::Value::String(value.to_string())
            }
        }
        FieldType::Float => {
            if let Ok(v) = value.parse::<f64>() {
                serde_json::Number::from_f64(v)
                    .map(serde_json::Value::Number)
                    .unwrap_or(serde_json::Value::String(value.to_string()))
            } else {
                serde_json::Value::String(value.to_string())
            }
        }
        FieldType::Decimal => {
            serde_json::Value::String(value.to_string())
        }
        FieldType::Bool => {
            let lower = value.to_lowercase();
            match lower.as_str() {
                "true" | "1" | "yes" | "on" | "t" | "y" => serde_json::Value::Bool(true),
                "false" | "0" | "no" | "off" | "f" | "n" => serde_json::Value::Bool(false),
                _ => serde_json::Value::String(value.to_string()),
            }
        }
        FieldType::Date | FieldType::DateTime => {
            serde_json::Value::String(value.to_string())
        }
        FieldType::Json => {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(value) {
                v
            } else {
                serde_json::Value::String(value.to_string())
            }
        }
        _ => serde_json::Value::String(value.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_csv_value_int() {
        let ft = FieldType::Int;
        assert_eq!(convert_csv_value("42", &ft), serde_json::Value::Number(42.into()));
        assert_eq!(convert_csv_value("", &ft), serde_json::Value::Null);
    }

    #[test]
    fn test_convert_csv_value_bool() {
        let ft = FieldType::Bool;
        assert_eq!(convert_csv_value("true", &ft), serde_json::Value::Bool(true));
        assert_eq!(convert_csv_value("1", &ft), serde_json::Value::Bool(true));
        assert_eq!(convert_csv_value("false", &ft), serde_json::Value::Bool(false));
        assert_eq!(convert_csv_value("0", &ft), serde_json::Value::Bool(false));
    }

    #[test]
    fn test_convert_csv_value_string() {
        let ft = FieldType::String;
        assert_eq!(
            convert_csv_value("hello", &ft),
            serde_json::Value::String("hello".to_string())
        );
    }
}
