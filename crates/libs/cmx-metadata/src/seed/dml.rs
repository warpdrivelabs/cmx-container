//! DML 语句生成模块
//!
//! 根据表定义和数据行，生成 PostgreSQL INSERT / UPSERT 语句。

use cmx_core::model::cell::{ColumnDefine, FieldType};
use crate::MetadataError;

/// 将 serde_json::Value 转义为 SQL 字面量
///
/// 对于时间类型字段，若字符串值为空，则返回 NULL 而非生成无效语法。
fn escape_sql_value(value: &serde_json::Value, field_type: &FieldType) -> String {
    match value {
        serde_json::Value::Null => "NULL".to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => {
            // DateTime 类型：数字被解析为 Number，需要转为 PostgreSQL 时间函数
            if matches!(field_type, FieldType::DateTime) {
                // Unix 时间戳（毫秒/秒）转为 TIMESTAMP
                if let Some(v) = n.as_i64() {
                    if v > 1_000_000_000_000 {
                        format!("to_timestamp({})", v as f64 / 1000.0)
                    } else {
                        format!("to_timestamp({})", v as f64)
                    }
                } else if let Some(v) = n.as_f64() {
                    format!("to_timestamp({})", v)
                } else {
                    n.to_string()
                }
            } else {
                n.to_string()
            }
        }
        serde_json::Value::String(s) => {
            // 空字符串对于时间类型返回 NULL，避免生成无效语法
            if s.is_empty() {
                return "NULL".to_string();
            }
            let escaped = escape_string(s);
            // 根据字段类型添加适当的 PostgreSQL 类型转换
            match field_type {
                FieldType::Date => format!("'{}'::date", escaped),
                FieldType::DateTime => format!("'{}'::timestamptz", escaped),
                _ => format!("'{}'", escaped),
            }
        }
        serde_json::Value::Array(arr) => {
            format!("'{}'", escape_string(&serde_json::to_string(arr).unwrap_or_default()))
        }
        serde_json::Value::Object(obj) => {
            if matches!(field_type, FieldType::Json) {
                format!("'{}'::jsonb", escape_string(&serde_json::to_string(obj).unwrap_or_default()))
            } else {
                format!("'{}'", escape_string(&serde_json::to_string(obj).unwrap_or_default()))
            }
        }
    }
}

/// 转义字符串中的单引号
fn escape_string(s: &str) -> String {
    s.replace('\'', "''")
}

/// 生成 PostgreSQL INSERT 或 UPSERT 语句（批量多行）
///
/// # 语句格式
/// ```sql
/// INSERT INTO table (col1, col2, ...) VALUES
/// (val1, val2, ...),
/// (val1, val2, ...),
/// ...
/// ```
///
/// # UPSERT 语义（当 conflict_columns 不为空时）
/// - 使用 `ON CONFLICT (conflict_cols) DO UPDATE SET` 实现
/// - 冲突列上存在冲突时，更新非冲突列（使用 EXCLUDED 表引用）
/// - 若 update_columns 为空，则 `DO NOTHING`
///
/// # 参数
/// * `table_name` - 表名
/// * `schema` - schema 名称（可选）
/// * `columns` - 表的列定义
/// * `rows` - 数据行列表
/// * `conflict_columns` - 冲突检测列（用于 ON CONFLICT 子句）
///
/// # 返回值
/// 成功返回完整的 SQL 语句
pub fn generate_pg_insert_or_upsert(
    table_name: &str,
    schema: Option<&str>,
    columns: &[ColumnDefine],
    rows: &[serde_json::Value],
    conflict_columns: &[String],
) -> Result<String, MetadataError> {
    if rows.is_empty() {
        return Err(MetadataError::SeedData("数据行为空，无法生成 DML 语句".to_string()));
    }

    // 构建带 schema 的表名
    let qualified_table = match schema {
        Some(s) if !s.is_empty() => format!("\"{}\".\"{}\"", s, table_name),
        _ => format!("\"{}\"", table_name),
    };

    // 收集所有需要插入的列（排除有默认值且数据中不存在的列）
    let insertable_columns: Vec<&ColumnDefine> = columns
        .iter()
        .filter(|col| {
            // 如果行数据中包含该列，则需要插入
            if rows.iter().any(|row| row.get(&col.name).is_some()) {
                return true;
            }
            // 行数据中不包含该列，但有默认值，则跳过（让数据库使用默认值）
            if col.default_value.is_some() {
                return false;
            }
            // 没有默认值，则插入 NULL（列必须可为空或主键有默认值）
            true
        })
        .collect();

    // 构建列名列表
    let column_names: Vec<&str> = insertable_columns.iter().map(|c| c.name.as_str()).collect();
    let col_list = column_names
        .iter()
        .map(|c| format!("\"{}\"", c))
        .collect::<Vec<_>>()
        .join(", ");

    // 构建 VALUES 子句
    let mut values_list = Vec::with_capacity(rows.len());
    for row in rows {
        let values: Vec<String> = insertable_columns
            .iter()
            .map(|col| {
                let value = row.get(&col.name).unwrap_or(&serde_json::Value::Null);
                escape_sql_value(value, &col.field_type)
            })
            .collect();
        values_list.push(format!("({})", values.join(", ")));
    }

    // 组装 INSERT 语句
    let mut sql = format!(
        "INSERT INTO {} ({}) VALUES\n{}",
        qualified_table,
        col_list,
        values_list.join(",\n")
    );

    // 添加 ON CONFLICT 子句（UPSERT）
    if !conflict_columns.is_empty() {
        let conflict_clause = conflict_columns
            .iter()
            .map(|c| format!("\"{}\"", c))
            .collect::<Vec<_>>()
            .join(", ");

        // 只更新插入的列，不更新跳过的列（有默认值的列）
        let update_columns: Vec<String> = insertable_columns
            .iter()
            .filter(|c| !conflict_columns.contains(&c.name))
            .map(|c| {
                format!("\"{0}\" = EXCLUDED.\"{0}\"", c.name)
            })
            .collect();

        if !update_columns.is_empty() {
            // 有非冲突列需要更新
            sql.push_str(&format!(
                "\nON CONFLICT ({}) DO UPDATE SET\n  {}",
                conflict_clause,
                update_columns.join(",\n  ")
            ));
        } else {
            // 所有列都是冲突列，只做冲突检测
            sql.push_str(&format!(
                "\nON CONFLICT ({}) DO NOTHING",
                conflict_clause
            ));
        }
    }

    Ok(sql)
}

/// 生成单行 PostgreSQL INSERT 或 UPSERT 语句（用于失败重试）
pub fn generate_pg_single_insert_or_upsert(
    table_name: &str,
    schema: Option<&str>,
    columns: &[ColumnDefine],
    row: &serde_json::Value,
    conflict_columns: &[String],
) -> Result<String, MetadataError> {
    generate_pg_insert_or_upsert(table_name, schema, columns, std::slice::from_ref(row), conflict_columns)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_col(name: &str, field_type: FieldType) -> ColumnDefine {
        ColumnDefine {
            name: name.to_string(),
            label: String::new(),
            field_type,
            is_primary_key: false,
            is_nullable: true,
            default_value: None,
            i18n: false,
            length: None,
            precision: None,
            scale: None,
            db_type: None,
            ordinal: None,
            create_time: None,
            update_time: None,
            is_foreign_key: false,
            foreign_key_table: None,
            foreign_key_column: None,
            extensions: std::collections::HashMap::new(),
        }
    }

    #[test]
    fn test_escape_string() {
        assert_eq!(escape_string("hello"), "hello");
        assert_eq!(escape_string("it's"), "it''s");
    }

    #[test]
    fn test_generate_simple_insert() {
        let columns = vec![
            make_col("id", FieldType::Int),
            make_col("name", FieldType::String),
        ];
        let rows = vec![
            serde_json::json!({"id": 1, "name": "测试"}),
        ];

        let sql = generate_pg_insert_or_upsert("test_table", None, &columns, &rows, &[]).unwrap();
        assert!(sql.contains("INSERT INTO \"test_table\""));
        assert!(sql.contains("\"id\", \"name\""));
        assert!(sql.contains("(1, '测试')"));
        assert!(!sql.contains("ON CONFLICT"));
    }

    #[test]
    fn test_generate_upsert() {
        let columns = vec![
            make_col("id", FieldType::Int),
            make_col("code", FieldType::String),
            make_col("name", FieldType::String),
        ];
        let rows = vec![
            serde_json::json!({"id": 1, "code": "FIN", "name": "财务域"}),
        ];
        let conflict_cols = vec!["code".to_string()];

        let sql = generate_pg_insert_or_upsert("test_table", None, &columns, &rows, &conflict_cols).unwrap();
        assert!(sql.contains("ON CONFLICT (\"code\")"));
        assert!(sql.contains("DO UPDATE SET"));
        assert!(sql.contains("\"id\" = EXCLUDED.\"id\""));
        assert!(sql.contains("\"name\" = EXCLUDED.\"name\""));
    }
}
