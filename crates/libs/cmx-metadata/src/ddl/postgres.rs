//! PostgreSQL DDL 方言实现
//!
//! 提供 `PostgresDdlDialect` 结构体，实现 `DdlDialect` trait，
//! 用于生成 PostgreSQL 数据库的 DDL 语句。
//!
//! # 功能特性
//! - 支持所有标准 SQL 数据类型映射
//! - 支持表空间设置
//! - 支持索引（普通索引、唯一索引）
//! - 支持表和列的注释
//! - 支持 round-trip 模式（保留原始 db_type）

use cmx_core::model::cell::{ColumnDefine, FieldType, IndexKind, TableDefine};
use crate::MetadataError;
use super::DdlDialect;

/// PostgreSQL DDL 方言
///
/// 用于生成 PostgreSQL 数据库的 DDL 语句。
/// 支持通过 `prefer_db_type` 字段开启 round-trip 模式。
///
/// # 示例
/// ```ignore
/// let dialect = PostgresDdlDialect::default();
/// let ddl = dialect.generate_create_table(&table)?;
/// ```
#[derive(Default)]
pub struct PostgresDdlDialect {
    /// 为 true 时优先使用 col.db_type（round-trip 模式）
    ///
    /// round-trip 模式下，会优先使用列定义中的 `db_type` 字段
    /// 而非根据 `FieldType` 自动映射类型。
    pub prefer_db_type: bool,
}

impl PostgresDdlDialect {
    /// 构造 schema 限定的表名
    fn qualified_table_name(&self, table_name: &str, schema: Option<&str>) -> String {
        match schema {
            Some(s) if !s.is_empty() => format!("\"{}\".\"{}\"", s, table_name),
            _ => format!("\"{}\"", table_name),
        }
    }

    /// 渲染单列定义
    fn render_column_def(&self, col: &ColumnDefine) -> String {
        let col_type = self.map_column_type(col);
        let nullable = if col.is_nullable { "" } else { " NOT NULL" };
        let default = match &col.default_value {
            Some(val) => format!(" DEFAULT {}", render_default_value(val)),
            None => String::new(),
        };
        format!("    \"{}\" {}{}{}", col.name, col_type, nullable, default)
    }
}

impl DdlDialect for PostgresDdlDialect {
    fn dialect_name(&self) -> &str {
        "PostgreSQL"
    }

    fn map_column_type(&self, col: &ColumnDefine) -> String {
        if self.prefer_db_type
            && let Some(ref db_type) = col.db_type
        {
            return db_type.clone();
        }
        match col.field_type {
            FieldType::String => match col.length {
                Some(len) => format!("VARCHAR({})", len),
                None => "TEXT".to_string(),
            },
            FieldType::Int => "BIGINT".to_string(),
            FieldType::Float => "DOUBLE PRECISION".to_string(),
            FieldType::Decimal => match (col.precision, col.scale) {
                (Some(p), Some(s)) => format!("NUMERIC({},{})", p, s),
                (Some(p), None) => format!("NUMERIC({})", p),
                _ => "NUMERIC".to_string(),
            },
            FieldType::DateTime => "TIMESTAMP WITH TIME ZONE".to_string(),
            FieldType::Date => "DATE".to_string(),
            FieldType::Bool => "BOOLEAN".to_string(),
            FieldType::Text => "TEXT".to_string(),
            FieldType::Binary => "BYTEA".to_string(),
            FieldType::Array => "JSONB".to_string(),
            FieldType::Json => "JSONB".to_string(),
            FieldType::Uuid => "UUID".to_string(),
            FieldType::Unknown => "TEXT".to_string(),


        }
    }

    fn generate_create_table(&self, table: &TableDefine) -> Result<String, MetadataError> {
        let qualified = self.qualified_table_name(&table.table_name, table.schema.as_deref());

        // 按 ordinal 排序列（若定义了 ordinal 字段），保证列顺序
        let mut sorted_cols: Vec<&ColumnDefine> = table.columns.iter().collect();
        sorted_cols.sort_by_key(|c| c.ordinal.unwrap_or(u32::MAX));

        // 生成列定义行
        let mut lines: Vec<String> = sorted_cols
            .iter()
            .map(|col| self.render_column_def(col))
            .collect();

        // ============================================
        // 构建 PRIMARY KEY 子句
        // ============================================
        // 优先使用显式定义的主键列，其次回退到列定义中标记为 is_primary_key 的列
        let pk_cols: Vec<String> = if !table.primary_keys.is_empty() {
            table.primary_keys.clone()
        } else {
            table.columns.iter()
                .filter(|c| c.is_primary_key)
                .map(|c| c.name.clone())
                .collect()
        };
        if !pk_cols.is_empty() {
            let pk_list = pk_cols.iter().map(|c| format!("\"{}\"", c)).collect::<Vec<_>>().join(", ");
            lines.push(format!("    PRIMARY KEY ({})", pk_list));
        }

        // 组装 CREATE TABLE 语句
        let mut ddl = format!("CREATE TABLE {} (\n{}\n)", qualified, lines.join(",\n"));

        // TABLESPACE：可选的表空间配置
        if let Some(ref ts) = table.tablespace {
            ddl.push_str(&format!(" TABLESPACE \"{}\"", ts));
        }

        ddl.push(';');
        Ok(ddl)
    }

    fn generate_create_indexes(&self, table: &TableDefine) -> Result<Vec<String>, MetadataError> {
        let qualified = self.qualified_table_name(&table.table_name, table.schema.as_deref());
        let mut stmts = Vec::new();
        for idx in &table.indexes {
            let cols = idx.columns.iter().map(|c| format!("\"{}\"", c)).collect::<Vec<_>>().join(", ");
            let unique = match idx.kind {
                IndexKind::Unique => "UNIQUE ",
                IndexKind::Normal => "",
            };
            stmts.push(format!("CREATE {}INDEX \"{}\" ON {} ({});", unique, idx.name, qualified, cols));
        }
        Ok(stmts)
    }

    fn generate_comments(&self, table: &TableDefine) -> Result<Vec<String>, MetadataError> {
        let qualified = self.qualified_table_name(&table.table_name, table.schema.as_deref());
        let mut stmts = Vec::new();

        // 表注释
        if let Some(ref comment) = table.comment {
            let escaped = comment.replace('\'', "''");
            stmts.push(format!("COMMENT ON TABLE {} IS '{}';", qualified, escaped));
        }

        // 列注释
        for col in &table.columns {
            if !col.label.is_empty() {
                let escaped = col.label.replace('\'', "''");
                stmts.push(format!(
                    "COMMENT ON COLUMN {}.\"{}\" IS '{}';",
                    qualified, col.name, escaped
                ));
            }
        }

        Ok(stmts)
    }

    fn generate_add_column(&self, table_name: &str, schema: Option<&str>, col: &ColumnDefine) -> Result<String, MetadataError> {
        let qualified = self.qualified_table_name(table_name, schema);
        let col_type = self.map_column_type(col);
        let nullable = if col.is_nullable { "" } else { " NOT NULL" };
        let default = match &col.default_value {
            Some(val) => format!(" DEFAULT {}", render_default_value(val)),
            None => String::new(),
        };
        Ok(format!(
            "ALTER TABLE {} ADD COLUMN \"{}\" {}{}{};",
            qualified, col.name, col_type, nullable, default
        ))
    }

    fn generate_drop_column(&self, table_name: &str, schema: Option<&str>, col_name: &str) -> Result<String, MetadataError> {
        let qualified = self.qualified_table_name(table_name, schema);
        Ok(format!("ALTER TABLE {} DROP COLUMN \"{}\";", qualified, col_name))
    }

    fn generate_alter_column(
        &self,
        table_name: &str,
        schema: Option<&str>,
        old_col: &ColumnDefine,
        new_col: &ColumnDefine,
    ) -> Result<Vec<String>, MetadataError> {
        let qualified = self.qualified_table_name(table_name, schema);
        let mut stmts = Vec::new();

        // ============================================
        // 变更1：类型变更
        // ============================================
        let old_type = self.map_column_type(old_col);
        let new_type = self.map_column_type(new_col);
        if old_type != new_type {
            stmts.push(format!(
                "ALTER TABLE {} ALTER COLUMN \"{}\" TYPE {};",
                qualified, new_col.name, new_type
            ));
        }

        // ============================================
        // 变更2：nullable 变更
        // ============================================
        if old_col.is_nullable != new_col.is_nullable {
            if new_col.is_nullable {
                // 从 NOT NULL 变为 NULLABLE
                stmts.push(format!(
                    "ALTER TABLE {} ALTER COLUMN \"{}\" DROP NOT NULL;",
                    qualified, new_col.name
                ));
            } else {
                // 从 NULLABLE 变为 NOT NULL
                stmts.push(format!(
                    "ALTER TABLE {} ALTER COLUMN \"{}\" SET NOT NULL;",
                    qualified, new_col.name
                ));
            }
        }

        // ============================================
        // 变更3：默认值变更
        // ============================================
        if old_col.default_value != new_col.default_value {
            match &new_col.default_value {
                Some(val) => {
                    stmts.push(format!(
                        "ALTER TABLE {} ALTER COLUMN \"{}\" SET DEFAULT {};",
                        qualified, new_col.name, render_default_value(val)
                    ));
                }
                None => {
                    // 移除默认值
                    stmts.push(format!(
                        "ALTER TABLE {} ALTER COLUMN \"{}\" DROP DEFAULT;",
                        qualified, new_col.name
                    ));
                }
            }
        }

        Ok(stmts)
    }

    fn generate_drop_table(&self, table: &TableDefine) -> Result<String, MetadataError> {
        let qualified = self.qualified_table_name(&table.table_name, table.schema.as_deref());
        Ok(format!("DROP TABLE IF EXISTS {} CASCADE;", qualified))
    }
}

/// 渲染默认值：数值/布尔/NULL/SQL函数直接输出，其他加单引号
fn render_default_value(val: &str) -> String {
    let trimmed = val.trim();
    // 数值
    if trimmed.parse::<f64>().is_ok() {
        return trimmed.to_string();
    }
    // 布尔 / NULL
    let upper = trimmed.to_uppercase();
    if matches!(upper.as_str(), "TRUE" | "FALSE" | "NULL") {
        return upper;
    }
    // SQL 函数（如 CURRENT_TIMESTAMP, NOW(), gen_random_uuid() 等）
    if upper.starts_with("CURRENT_")
        || upper.starts_with("NOW")
        || upper.starts_with("GEN_")
        || upper.contains('(')
    {
        return trimmed.to_string();
    }
    // 其他字符串
    format!("'{}'", trimmed.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use cmx_core::model::cell::IndexDefine;

    fn make_col(name: &str, label: &str, ft: FieldType, nullable: bool) -> ColumnDefine {
        ColumnDefine {
            name: name.to_string(),
            label: label.to_string(),
            field_type: ft,
            is_primary_key: false,
            is_nullable: nullable,
            default_value: None,
            i18n: false,
            length: None, precision: None, scale: None,
            db_type: None, ordinal: None,
            create_time: None, update_time: None,
            is_foreign_key: false,
            foreign_key_table: None, foreign_key_column: None,
            extensions: HashMap::new(),
        }
    }

    #[test]
    fn test_type_mapping() {
        let dialect = PostgresDdlDialect::default();

        let mut col = make_col("c", "", FieldType::String, true);
        col.length = Some(32);
        assert_eq!(dialect.map_column_type(&col), "VARCHAR(32)");

        col.length = None;
        assert_eq!(dialect.map_column_type(&col), "TEXT");

        col.field_type = FieldType::Int;
        assert_eq!(dialect.map_column_type(&col), "BIGINT");

        col.field_type = FieldType::Float;
        assert_eq!(dialect.map_column_type(&col), "DOUBLE PRECISION");

        col.field_type = FieldType::Decimal;
        col.precision = Some(10);
        col.scale = Some(2);
        assert_eq!(dialect.map_column_type(&col), "NUMERIC(10,2)");

        col.field_type = FieldType::DateTime;
        assert_eq!(dialect.map_column_type(&col), "TIMESTAMP WITH TIME ZONE");

        col.field_type = FieldType::Date;
        assert_eq!(dialect.map_column_type(&col), "DATE");

        col.field_type = FieldType::Bool;
        assert_eq!(dialect.map_column_type(&col), "BOOLEAN");

        col.field_type = FieldType::Text;
        assert_eq!(dialect.map_column_type(&col), "TEXT");

        col.field_type = FieldType::Binary;
        assert_eq!(dialect.map_column_type(&col), "BYTEA");

        col.field_type = FieldType::Array;
        assert_eq!(dialect.map_column_type(&col), "JSONB");

        col.field_type = FieldType::Json;
        assert_eq!(dialect.map_column_type(&col), "JSONB");

        col.field_type = FieldType::Uuid;
        assert_eq!(dialect.map_column_type(&col), "UUID");
    }

    #[test]
    fn test_prefer_db_type() {
        let dialect = PostgresDdlDialect { prefer_db_type: true };
        let mut col = make_col("c", "", FieldType::String, true);
        col.db_type = Some("VARCHAR2(30)".to_string());
        assert_eq!(dialect.map_column_type(&col), "VARCHAR2(30)");
    }

    #[test]
    fn test_create_table() {
        let dialect = PostgresDdlDialect::default();
        let table = TableDefine {
            table_name: "cmx_domain".to_string(),
            display_name: "域".to_string(),
            columns: vec![
                {
                    let mut c = make_col("id", "主键", FieldType::Int, false);
                    c.is_primary_key = true;
                    c.ordinal = Some(1);
                    c
                },
                {
                    let mut c = make_col("code", "域编码", FieldType::String, false);
                    c.length = Some(32);
                    c.ordinal = Some(2);
                    c
                },
                {
                    let mut c = make_col("sort_order", "排序号", FieldType::Int, true);
                    c.default_value = Some("0".to_string());
                    c.ordinal = Some(3);
                    c
                },
            ],
            primary_keys: vec!["id".to_string()],
            indexes: vec![],
            version: 1,
            create_time: None, update_time: None,
            i18n: false,
            comment: Some("企业应用域".to_string()),
            schema: Some("public".to_string()),
            tablespace: None,
            is_partitioned: false,
            partition_type: None,
            partition_columns: vec![],
            extensions: HashMap::new(),
        };

        let ddl = dialect.generate_create_table(&table).unwrap();
        assert!(ddl.contains("\"public\".\"cmx_domain\""));
        assert!(ddl.contains("\"id\" BIGINT NOT NULL"));
        assert!(ddl.contains("\"code\" VARCHAR(32) NOT NULL"));
        assert!(ddl.contains("\"sort_order\" BIGINT DEFAULT 0"));
        assert!(ddl.contains("PRIMARY KEY (\"id\")"));
    }

    #[test]
    fn test_create_indexes() {
        let dialect = PostgresDdlDialect::default();
        let table = TableDefine {
            table_name: "cmx_domain".to_string(),
            display_name: "域".to_string(),
            columns: vec![],
            primary_keys: vec![],
            indexes: vec![
                IndexDefine {
                    name: "uk_cmx_domain_code".to_string(),
                    columns: vec!["code".to_string()],
                    kind: IndexKind::Unique,
                },
                IndexDefine {
                    name: "idx_cmx_domain_type".to_string(),
                    columns: vec!["type".to_string()],
                    kind: IndexKind::Normal,
                },
            ],
            version: 1,
            create_time: None, update_time: None,
            i18n: false,
            comment: None, schema: None, tablespace: None,
            is_partitioned: false,
            partition_type: None,
            partition_columns: vec![],
            extensions: HashMap::new(),
        };

        let stmts = dialect.generate_create_indexes(&table).unwrap();
        assert_eq!(stmts.len(), 2);
        assert!(stmts[0].contains("CREATE UNIQUE INDEX"));
        assert!(stmts[1].contains("CREATE INDEX"));
    }

    #[test]
    fn test_comments() {
        let dialect = PostgresDdlDialect::default();
        let table = TableDefine {
            table_name: "cmx_domain".to_string(),
            display_name: "域".to_string(),
            columns: vec![
                make_col("id", "主键", FieldType::Int, false),
                make_col("code", "域编码", FieldType::String, false),
            ],
            primary_keys: vec![],
            indexes: vec![],
            version: 1,
            create_time: None, update_time: None,
            i18n: false,
            comment: Some("企业应用域".to_string()),
            schema: Some("public".to_string()),
            tablespace: None,
            is_partitioned: false,
            partition_type: None,
            partition_columns: vec![],
            extensions: HashMap::new(),
        };

        let stmts = dialect.generate_comments(&table).unwrap();
        assert_eq!(stmts.len(), 3); // 1 table + 2 columns
        assert!(stmts[0].contains("COMMENT ON TABLE"));
        assert!(stmts[1].contains("COMMENT ON COLUMN"));
    }

    #[test]
    fn test_default_value_rendering() {
        assert_eq!(render_default_value("0"), "0");
        assert_eq!(render_default_value("3.14"), "3.14");
        assert_eq!(render_default_value("true"), "TRUE");
        assert_eq!(render_default_value("false"), "FALSE");
        assert_eq!(render_default_value("NULL"), "NULL");
        assert_eq!(render_default_value("CURRENT_TIMESTAMP"), "CURRENT_TIMESTAMP");
        assert_eq!(render_default_value("now()"), "now()");
        assert_eq!(render_default_value("active"), "'active'");
        assert_eq!(render_default_value("it's"), "'it''s'");
    }

    #[test]
    fn test_full_ddl() {
        let dialect = PostgresDdlDialect::default();
        let table = TableDefine {
            table_name: "test_table".to_string(),
            display_name: "测试表".to_string(),
            columns: vec![
                {
                    let mut c = make_col("id", "主键", FieldType::Int, false);
                    c.is_primary_key = true;
                    c
                },
                make_col("name", "名称", FieldType::String, false),
            ],
            primary_keys: vec!["id".to_string()],
            indexes: vec![
                IndexDefine {
                    name: "idx_test_name".to_string(),
                    columns: vec!["name".to_string()],
                    kind: IndexKind::Normal,
                },
            ],
            version: 1,
            create_time: None, update_time: None,
            i18n: false,
            comment: Some("测试表".to_string()),
            schema: None, tablespace: None,
            is_partitioned: false,
            partition_type: None,
            partition_columns: vec![],
            extensions: HashMap::new(),
        };

        let ddl = dialect.generate_full_ddl(&table).unwrap();
        assert!(ddl.contains("CREATE TABLE"));
        assert!(ddl.contains("COMMENT ON TABLE"));
        assert!(ddl.contains("COMMENT ON COLUMN"));
        assert!(ddl.contains("CREATE INDEX"));
    }

    #[test]
    fn test_drop_table() {
        let dialect = PostgresDdlDialect::default();
        let table = TableDefine {
            table_name: "test".to_string(),
            display_name: "".to_string(),
            columns: vec![],
            primary_keys: vec![],
            indexes: vec![],
            version: 1,
            create_time: None, update_time: None,
            i18n: false,
            comment: None,
            schema: Some("public".to_string()),
            tablespace: None,
            is_partitioned: false,
            partition_type: None,
            partition_columns: vec![],
            extensions: HashMap::new(),
        };
        let ddl = dialect.generate_drop_table(&table).unwrap();
        assert_eq!(ddl, "DROP TABLE IF EXISTS \"public\".\"test\" CASCADE;");
    }

    #[test]
    fn test_add_column() {
        let dialect = PostgresDdlDialect::default();
        let col = make_col("status", "状态", FieldType::String, true);
        let ddl = dialect.generate_add_column("cmx_domain", Some("public"), &col).unwrap();
        assert!(ddl.contains("ALTER TABLE"));
        assert!(ddl.contains("ADD COLUMN"));
        assert!(ddl.contains("\"status\" TEXT"));
    }

    #[test]
    fn test_alter_column() {
        let dialect = PostgresDdlDialect::default();
        let mut old = make_col("name", "名称", FieldType::String, true);
        old.length = Some(64); // VARCHAR(64)
        let mut new = make_col("name", "名称", FieldType::Text, false);
        new.default_value = Some("unnamed".to_string());

        let stmts = dialect.generate_alter_column("test", None, &old, &new).unwrap();
        assert_eq!(stmts.len(), 3); // TYPE + SET NOT NULL + SET DEFAULT
        assert!(stmts[0].contains("TYPE TEXT"));
        assert!(stmts[1].contains("SET NOT NULL"));
        assert!(stmts[2].contains("SET DEFAULT 'unnamed'"));
    }

    #[test]
    fn test_tablespace() {
        let dialect = PostgresDdlDialect::default();
        let table = TableDefine {
            table_name: "test".to_string(),
            display_name: "".to_string(),
            columns: vec![
                {
                    let mut c = make_col("id", "", FieldType::Int, false);
                    c.is_primary_key = true;
                    c
                },
            ],
            primary_keys: vec!["id".to_string()],
            indexes: vec![],
            version: 1,
            create_time: None, update_time: None,
            i18n: false,
            comment: None, schema: None,
            tablespace: Some("my_tablespace".to_string()),
            is_partitioned: false,
            partition_type: None,
            partition_columns: vec![],
            extensions: HashMap::new(),
        };
        let ddl = dialect.generate_create_table(&table).unwrap();
        assert!(ddl.contains("TABLESPACE \"my_tablespace\""));
    }
}
