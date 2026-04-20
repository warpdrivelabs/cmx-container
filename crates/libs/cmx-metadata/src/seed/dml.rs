//! DML 语句生成模块
//!
//! 根据表定义和数据行，生成 PostgreSQL INSERT / UPSERT 语句。

use cmx_core::model::cell::{ColumnDefine, FieldType};
use crate::MetadataError;

/// 将 serde_json::Value 转义为 SQL 字面量
fn escape_sql_value(value: &serde_json::Value, field_type: &FieldType) -> String {
    match value {
        serde_json::Value::Null => "NULL".to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => format!("'{}'", escape_string(s)),
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
/// # 参数
/// - `table_name`: 表名
/// - `schema`: schema 名称（可选）
/// - `columns`: 表的列定义
/// - `rows`: 数据行列表
/// - `conflict_columns`: 冲突检测列（用于 ON CONFLICT 子句）
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

    let qualified_table = match schema {
        Some(s) if !s.is_empty() => format!("\"{}\".\"{}\"", s, table_name),
        _ => format!("\"{}\"", table_name),
    };

    let column_names: Vec<&str> = columns.iter().map(|c| c.name.as_str()).collect();

    let col_list = column_names
        .iter()
        .map(|c| format!("\"{}\"", c))
        .collect::<Vec<_>>()
        .join(", ");

    let mut values_list = Vec::with_capacity(rows.len());

    for row in rows {
        let values: Vec<String> = columns
            .iter()
            .map(|col| {
                let value = row.get(&col.name).unwrap_or(&serde_json::Value::Null);
                escape_sql_value(value, &col.field_type)
            })
            .collect();
        values_list.push(format!("({})", values.join(", ")));
    }

    let mut sql = format!(
        "INSERT INTO {} ({}) VALUES\n{}",
        qualified_table,
        col_list,
        values_list.join(",\n")
    );

    if !conflict_columns.is_empty() {
        let conflict_clause = conflict_columns
            .iter()
            .map(|c| format!("\"{}\"", c))
            .collect::<Vec<_>>()
            .join(", ");

        let update_columns: Vec<String> = columns
            .iter()
            .filter(|c| !conflict_columns.contains(&c.name))
            .map(|c| {
                format!("\"{0}\" = EXCLUDED.\"{0}\"", c.name)
            })
            .collect();

        if !update_columns.is_empty() {
            sql.push_str(&format!(
                "\nON CONFLICT ({}) DO UPDATE SET\n  {}",
                conflict_clause,
                update_columns.join(",\n  ")
            ));
        } else {
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
