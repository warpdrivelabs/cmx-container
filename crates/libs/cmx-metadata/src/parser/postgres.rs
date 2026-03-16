//! PostgreSQL DDL 解析器实现

use std::collections::HashMap;

use regex::Regex;

use cmx_core::model::cell::{ColumnDefine, FieldType, IndexDefine, IndexKind, TableDefine};
use crate::MetadataError;
use super::DdlParser;

pub struct PostgresDdlParser;

impl DdlParser for PostgresDdlParser {
    fn dialect_name(&self) -> &str {
        "PostgreSQL"
    }

    fn parse_create_table(&self, ddl: &str) -> Result<TableDefine, MetadataError> {
        let tables = self.parse_ddl(ddl)?;
        tables
            .into_iter()
            .next()
            .ok_or_else(|| MetadataError::DdlParse("未找到 CREATE TABLE 语句".to_string()))
    }

    fn parse_ddl(&self, ddl: &str) -> Result<Vec<TableDefine>, MetadataError> {
        let statements = split_statements(ddl);
        let mut tables: HashMap<String, TableDefine> = HashMap::new();

        // 第一遍：解析 CREATE TABLE
        for stmt in &statements {
            let trimmed = stmt.trim();
            if is_create_table(trimmed) {
                let table = parse_create_table_stmt(trimmed)?;
                tables.insert(table.table_name.clone(), table);
            }
        }

        // 第二遍：解析 CREATE INDEX 和 COMMENT ON
        for stmt in &statements {
            let trimmed = stmt.trim();
            if is_create_index(trimmed) {
                if let Some((table_name, idx)) = parse_create_index_stmt(trimmed)
                    && let Some(table) = tables.get_mut(&table_name)
                {
                    table.indexes.push(idx);
                }
            } else if is_comment_on(trimmed) {
                apply_comment(trimmed, &mut tables);
            }
        }

        // 按插入顺序返回（使用原 DDL 中 CREATE TABLE 的顺序）
        let mut result = Vec::new();
        for stmt in &statements {
            let trimmed = stmt.trim();
            if is_create_table(trimmed)
                && let Ok(temp) = parse_create_table_stmt(trimmed)
                && let Some(table) = tables.remove(&temp.table_name)
            {
                result.push(table);
            }
        }
        // 添加任何剩余的（不应该有，但以防万一）
        for (_, table) in tables {
            result.push(table);
        }

        Ok(result)
    }
}

/// 按 ; 分割 SQL 语句
fn split_statements(ddl: &str) -> Vec<String> {
    ddl.split(';')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn is_create_table(stmt: &str) -> bool {
    let upper = stmt.to_uppercase();
    upper.starts_with("CREATE TABLE")
}

fn is_create_index(stmt: &str) -> bool {
    let upper = stmt.to_uppercase();
    upper.starts_with("CREATE INDEX") || upper.starts_with("CREATE UNIQUE INDEX")
}

fn is_comment_on(stmt: &str) -> bool {
    let upper = stmt.to_uppercase();
    upper.starts_with("COMMENT ON")
}

/// 解析 CREATE TABLE 语句
fn parse_create_table_stmt(stmt: &str) -> Result<TableDefine, MetadataError> {
    // 提取表名（支持 schema.table 和 引号格式）
    let re_table = Regex::new(
        r#"(?i)CREATE\s+TABLE\s+(?:IF\s+NOT\s+EXISTS\s+)?(?:"?(\w+)"?\.)?"?(\w+)"?\s*\("#
    ).unwrap();

    let caps = re_table.captures(stmt).ok_or_else(|| {
        MetadataError::DdlParse(format!("无法解析 CREATE TABLE 语句: {}", &stmt[..stmt.len().min(100)]))
    })?;

    let schema = caps.get(1).map(|m| m.as_str().to_string());
    let table_name = caps[2].to_string();

    // 提取括号内的内容
    let paren_start = stmt.find('(').ok_or_else(|| {
        MetadataError::DdlParse("CREATE TABLE 缺少左括号".to_string())
    })?;

    // 找到匹配的右括号（考虑嵌套）
    let body = find_matching_paren(&stmt[paren_start..])?;

    // 分割列定义和约束
    let parts = split_column_defs(body);

    let mut columns = Vec::new();
    let mut primary_keys = Vec::new();
    let mut ordinal: u32 = 1;

    for part in &parts {
        let trimmed = part.trim();
        let upper = trimmed.to_uppercase();

        if upper.starts_with("PRIMARY KEY") {
            // 提取 PRIMARY KEY 列
            if let Some(cols) = extract_paren_columns(trimmed) {
                primary_keys = cols;
            }
        } else if upper.starts_with("CONSTRAINT")
            || upper.starts_with("FOREIGN KEY")
            || upper.starts_with("UNIQUE")
            || upper.starts_with("CHECK")
        {
            // 跳过其他约束
        } else {
            // 列定义
            if let Some(mut col) = parse_column_def(trimmed) {
                col.ordinal = Some(ordinal);
                ordinal += 1;
                columns.push(col);
            }
        }
    }

    // 标记主键列
    for col in &mut columns {
        if primary_keys.contains(&col.name) {
            col.is_primary_key = true;
        }
    }

    Ok(TableDefine {
        table_name,
        display_name: String::new(),
        columns,
        primary_keys,
        indexes: vec![],
        version: 1,
        created_at: None,
        updated_at: None,
        i18n: false,
        comment: None,
        schema,
        tablespace: None,
        is_partitioned: false,
        partition_type: None,
        partition_columns: vec![],
        extensions: HashMap::new(),
    })
}

/// 找到匹配的右括号，返回括号内的内容
fn find_matching_paren(s: &str) -> Result<&str, MetadataError> {
    let mut depth = 0;
    let mut start = None;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' => {
                if depth == 0 {
                    start = Some(i + 1);
                }
                depth += 1;
            }
            ')' => {
                depth -= 1;
                if depth == 0 {
                    let s_start = start.unwrap();
                    return Ok(&s[s_start..i]);
                }
            }
            _ => {}
        }
    }
    Err(MetadataError::DdlParse("括号不匹配".to_string()))
}

/// 按逗号分割列定义（考虑嵌套括号）
fn split_column_defs(body: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut depth = 0;

    for ch in body.chars() {
        match ch {
            '(' => {
                depth += 1;
                current.push(ch);
            }
            ')' => {
                depth -= 1;
                current.push(ch);
            }
            ',' if depth == 0 => {
                parts.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    if !current.trim().is_empty() {
        parts.push(current.trim().to_string());
    }
    parts
}

/// 从括号中提取列名列表
fn extract_paren_columns(s: &str) -> Option<Vec<String>> {
    let start = s.find('(')?;
    let end = s.rfind(')')?;
    let inner = &s[start + 1..end];
    Some(
        inner
            .split(',')
            .map(|c| c.trim().trim_matches('"').to_string())
            .filter(|c| !c.is_empty())
            .collect(),
    )
}

/// 解析单列定义
fn parse_column_def(def: &str) -> Option<ColumnDefine> {
    let tokens = tokenize_column_def(def);
    if tokens.is_empty() {
        return None;
    }

    // 第一个 token 是列名
    let name = tokens[0].trim_matches('"').to_string();

    // 跳过看起来不像列定义的行
    if name.to_uppercase() == "PRIMARY"
        || name.to_uppercase() == "CONSTRAINT"
        || name.to_uppercase() == "FOREIGN"
        || name.to_uppercase() == "UNIQUE"
        || name.to_uppercase() == "CHECK"
    {
        return None;
    }

    // 解析类型（可能是多个 token，如 "DOUBLE PRECISION"、"TIMESTAMP WITH TIME ZONE"）
    let (field_type, length, precision, scale, db_type, type_end_idx) = parse_pg_type(&tokens[1..]);

    // 解析修饰符（NOT NULL, DEFAULT）
    let remaining = &tokens[1 + type_end_idx..];
    let is_not_null = remaining.windows(2).any(|w| {
        w[0].to_uppercase() == "NOT" && w[1].to_uppercase() == "NULL"
    });

    let default_value = extract_default_value(remaining);

    Some(ColumnDefine {
        name,
        label: String::new(),
        field_type,
        is_primary_key: false,
        is_nullable: !is_not_null,
        default_value,
        i18n: false,
        length,
        precision,
        scale,
        db_type: Some(db_type),
        ordinal: None,
        created_at: None,
        updated_at: None,
        is_foreign_key: false,
        foreign_key_table: None,
        foreign_key_column: None,
        extensions: HashMap::new(),
    })
}

/// 将列定义拆分为 tokens（尊重引号和括号）
fn tokenize_column_def(def: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut in_parens = 0;

    for ch in def.chars() {
        match ch {
            '"' => {
                in_quotes = !in_quotes;
                current.push(ch);
            }
            '(' if !in_quotes => {
                in_parens += 1;
                current.push(ch);
            }
            ')' if !in_quotes => {
                in_parens -= 1;
                current.push(ch);
            }
            ' ' | '\t' | '\n' | '\r' if !in_quotes && in_parens == 0 => {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }
            }
            _ => current.push(ch),
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

/// 解析 PG 类型字符串 → (FieldType, length, precision, scale, db_type_string, consumed_tokens)
fn parse_pg_type(tokens: &[String]) -> (FieldType, Option<u32>, Option<u32>, Option<u32>, String, usize) {
    if tokens.is_empty() {
        return (FieldType::String, None, None, None, "TEXT".to_string(), 0);
    }

    let first = tokens[0].to_uppercase();
    let first_base = first.split('(').next().unwrap_or(&first);

    match first_base {
        "VARCHAR" | "CHARACTER" => {
            let len = extract_single_param(&tokens[0]);
            let mut consumed = 1;
            // CHARACTER VARYING(n)
            if first_base == "CHARACTER" && tokens.len() > 1 && tokens[1].to_uppercase().starts_with("VARYING") {
                let len2 = extract_single_param(&tokens[1]);
                consumed = 2;
                let l = len2.or(len);
                let db = if let Some(l) = l { format!("VARCHAR({})", l) } else { "VARCHAR".to_string() };
                return (FieldType::String, l, None, None, db, consumed);
            }
            let db = if let Some(l) = len { format!("VARCHAR({})", l) } else { "VARCHAR".to_string() };
            (FieldType::String, len, None, None, db, consumed)
        }
        "TEXT" => (FieldType::Text, None, None, None, "TEXT".to_string(), 1),
        "BIGINT" | "INT8" => (FieldType::Int, None, None, None, "BIGINT".to_string(), 1),
        "INTEGER" | "INT" | "INT4" => (FieldType::Int, None, None, None, "INTEGER".to_string(), 1),
        "SMALLINT" | "INT2" => (FieldType::Int, None, None, None, "SMALLINT".to_string(), 1),
        "SERIAL" => (FieldType::Int, None, None, None, "SERIAL".to_string(), 1),
        "BIGSERIAL" => (FieldType::Int, None, None, None, "BIGSERIAL".to_string(), 1),
        "DOUBLE" => {
            // DOUBLE PRECISION
            let consumed = if tokens.len() > 1 && tokens[1].to_uppercase() == "PRECISION" { 2 } else { 1 };
            (FieldType::Float, None, None, None, "DOUBLE PRECISION".to_string(), consumed)
        }
        "FLOAT8" => (FieldType::Float, None, None, None, "DOUBLE PRECISION".to_string(), 1),
        "REAL" | "FLOAT4" => (FieldType::Float, None, None, None, "REAL".to_string(), 1),
        "NUMERIC" | "DECIMAL" => {
            let (p, s) = extract_precision_scale(&tokens[0]);
            let db = match (p, s) {
                (Some(p), Some(s)) => format!("NUMERIC({},{})", p, s),
                (Some(p), None) => format!("NUMERIC({})", p),
                _ => "NUMERIC".to_string(),
            };
            (FieldType::Decimal, None, p, s, db, 1)
        }
        "BOOLEAN" | "BOOL" => (FieldType::Bool, None, None, None, "BOOLEAN".to_string(), 1),
        "TIMESTAMP" | "TIMESTAMPTZ" => {
            // TIMESTAMP WITH TIME ZONE / TIMESTAMP WITHOUT TIME ZONE / TIMESTAMPTZ
            let mut consumed = 1;
            if first_base == "TIMESTAMP"
                && tokens.len() > 3
                && (tokens[1].to_uppercase() == "WITH" || tokens[1].to_uppercase() == "WITHOUT")
                && tokens[2].to_uppercase() == "TIME"
                && tokens[3].to_uppercase() == "ZONE"
            {
                consumed = 4;
            }
            (FieldType::DateTime, None, None, None, "TIMESTAMP WITH TIME ZONE".to_string(), consumed)
        }
        "DATE" => (FieldType::Date, None, None, None, "DATE".to_string(), 1),
        "BYTEA" => (FieldType::Binary, None, None, None, "BYTEA".to_string(), 1),
        "JSONB" => (FieldType::Json, None, None, None, "JSONB".to_string(), 1),
        "JSON" => (FieldType::Json, None, None, None, "JSON".to_string(), 1),
        "UUID" => (FieldType::Uuid, None, None, None, "UUID".to_string(), 1),
        _ => {
            // 未识别的类型，保存原始 db_type
            (FieldType::String, None, None, None, tokens[0].clone(), 1)
        }
    }
}

/// 从类型括号中提取单个参数，如 VARCHAR(32) → Some(32)
fn extract_single_param(token: &str) -> Option<u32> {
    let start = token.find('(')?;
    let end = token.find(')')?;
    token[start + 1..end].trim().parse().ok()
}

/// 从类型括号中提取精度和小数位，如 NUMERIC(10,2) → (Some(10), Some(2))
fn extract_precision_scale(token: &str) -> (Option<u32>, Option<u32>) {
    if let Some(start) = token.find('(')
        && let Some(end) = token.find(')')
    {
        let inner = &token[start + 1..end];
        let parts: Vec<&str> = inner.split(',').collect();
        let p = parts.first().and_then(|s| s.trim().parse().ok());
        let s = parts.get(1).and_then(|s| s.trim().parse().ok());
        return (p, s);
    }
    (None, None)
}

/// 从修饰符 tokens 中提取 DEFAULT 值
fn extract_default_value(tokens: &[String]) -> Option<String> {
    for (i, t) in tokens.iter().enumerate() {
        if t.to_uppercase() == "DEFAULT" {
            // DEFAULT 后面的内容就是默认值（可能含括号如 now()）
            if i + 1 < tokens.len() {
                // 收集到下一个关键字或结尾
                let mut val_parts = Vec::new();
                for token in &tokens[i + 1..] {
                    let u = token.to_uppercase();
                    if u == "NOT" || u == "NULL" || u == "PRIMARY" || u == "REFERENCES" || u == "CHECK" || u == "CONSTRAINT" {
                        break;
                    }
                    val_parts.push(token.clone());
                }
                if !val_parts.is_empty() {
                    let val = val_parts.join(" ");
                    // 去掉外层单引号
                    let trimmed = val.trim_matches('\'');
                    return Some(trimmed.to_string());
                }
            }
        }
    }
    None
}

/// 解析 CREATE INDEX 语句
fn parse_create_index_stmt(stmt: &str) -> Option<(String, IndexDefine)> {
    let re = Regex::new(
        r#"(?i)CREATE\s+(UNIQUE\s+)?INDEX\s+(?:IF\s+NOT\s+EXISTS\s+)?"?(\w+)"?\s+ON\s+(?:"?\w+"?\.)?"?(\w+)"?\s*\(([^)]+)\)"#
    ).ok()?;

    let caps = re.captures(stmt)?;
    let is_unique = caps.get(1).is_some();
    let index_name = caps[2].to_string();
    let table_name = caps[3].to_string();
    let columns: Vec<String> = caps[4]
        .split(',')
        .map(|c| c.trim().trim_matches('"').to_string())
        .filter(|c| !c.is_empty())
        .collect();

    let kind = if is_unique { IndexKind::Unique } else { IndexKind::Normal };

    Some((
        table_name,
        IndexDefine {
            name: index_name,
            columns,
            kind,
        },
    ))
}

/// 解析 COMMENT ON 语句并应用到对应的 TableDefine
fn apply_comment(stmt: &str, tables: &mut HashMap<String, TableDefine>) {
    // COMMENT ON TABLE
    let re_table = Regex::new(
        r#"(?i)COMMENT\s+ON\s+TABLE\s+(?:"?\w+"?\.)?"?(\w+)"?\s+IS\s+'((?:[^']|'')*)'?"#
    ).ok();
    if let Some(re) = &re_table
        && let Some(caps) = re.captures(stmt)
    {
        let table_name = caps[1].to_string();
        let comment = caps[2].replace("''", "'");
        if let Some(table) = tables.get_mut(&table_name) {
            table.comment = Some(comment);
        }
        return;
    }

    // COMMENT ON COLUMN
    let re_col = Regex::new(
        r#"(?i)COMMENT\s+ON\s+COLUMN\s+(?:"?\w+"?\.)?"?(\w+)"?\."?(\w+)"?\s+IS\s+'((?:[^']|'')*)'?"#
    ).ok();
    if let Some(re) = &re_col
        && let Some(caps) = re.captures(stmt)
    {
        let table_name = caps[1].to_string();
        let col_name = caps[2].to_string();
        let label = caps[3].replace("''", "'");
        if let Some(table) = tables.get_mut(&table_name)
            && let Some(col) = table.columns.iter_mut().find(|c| c.name == col_name)
        {
            col.label = label;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_create_table() {
        let ddl = r#"
            CREATE TABLE "cmx_domain" (
                "id" BIGINT NOT NULL,
                "code" VARCHAR(32) NOT NULL,
                "name" VARCHAR(64) NOT NULL,
                "description" TEXT,
                "sort_order" BIGINT DEFAULT 0,
                PRIMARY KEY ("id")
            )
        "#;
        let parser = PostgresDdlParser;
        let table = parser.parse_create_table(ddl).unwrap();
        assert_eq!(table.table_name, "cmx_domain");
        assert_eq!(table.columns.len(), 5);
        assert_eq!(table.primary_keys, vec!["id"]);

        let id_col = &table.columns[0];
        assert_eq!(id_col.name, "id");
        assert!(!id_col.is_nullable);
        assert!(id_col.is_primary_key);

        let code_col = &table.columns[1];
        assert_eq!(code_col.name, "code");
        assert_eq!(code_col.field_type, FieldType::String);
        assert_eq!(code_col.length, Some(32));

        let sort_col = &table.columns[4];
        assert_eq!(sort_col.default_value, Some("0".to_string()));
    }

    #[test]
    fn test_parse_with_schema() {
        let ddl = r#"CREATE TABLE "public"."test" ("id" BIGINT NOT NULL, PRIMARY KEY ("id"))"#;
        let parser = PostgresDdlParser;
        let table = parser.parse_create_table(ddl).unwrap();
        assert_eq!(table.table_name, "test");
        assert_eq!(table.schema, Some("public".to_string()));
    }

    #[test]
    fn test_parse_all_pg_types() {
        let ddl = r#"
            CREATE TABLE "type_test" (
                "c_bigint" BIGINT,
                "c_int" INTEGER,
                "c_smallint" SMALLINT,
                "c_double" DOUBLE PRECISION,
                "c_real" REAL,
                "c_numeric" NUMERIC(10,2),
                "c_bool" BOOLEAN,
                "c_timestamp" TIMESTAMP WITH TIME ZONE,
                "c_timestamptz" TIMESTAMPTZ,
                "c_date" DATE,
                "c_text" TEXT,
                "c_varchar" VARCHAR(100),
                "c_bytea" BYTEA,
                "c_jsonb" JSONB,
                "c_json" JSON,
                "c_uuid" UUID
            )
        "#;
        let parser = PostgresDdlParser;
        let table = parser.parse_create_table(ddl).unwrap();
        assert_eq!(table.columns.len(), 16);

        assert_eq!(table.columns[0].field_type, FieldType::Int);     // BIGINT
        assert_eq!(table.columns[1].field_type, FieldType::Int);     // INTEGER
        assert_eq!(table.columns[2].field_type, FieldType::Int);     // SMALLINT
        assert_eq!(table.columns[3].field_type, FieldType::Float);   // DOUBLE PRECISION
        assert_eq!(table.columns[4].field_type, FieldType::Float);   // REAL
        assert_eq!(table.columns[5].field_type, FieldType::Decimal); // NUMERIC
        assert_eq!(table.columns[5].precision, Some(10));
        assert_eq!(table.columns[5].scale, Some(2));
        assert_eq!(table.columns[6].field_type, FieldType::Bool);     // BOOLEAN
        assert_eq!(table.columns[7].field_type, FieldType::DateTime); // TIMESTAMP WITH TZ
        assert_eq!(table.columns[8].field_type, FieldType::DateTime); // TIMESTAMPTZ
        assert_eq!(table.columns[9].field_type, FieldType::Date);     // DATE
        assert_eq!(table.columns[10].field_type, FieldType::Text);    // TEXT
        assert_eq!(table.columns[11].field_type, FieldType::String);  // VARCHAR
        assert_eq!(table.columns[11].length, Some(100));
        assert_eq!(table.columns[12].field_type, FieldType::Binary);  // BYTEA
        assert_eq!(table.columns[13].field_type, FieldType::Json);    // JSONB
        assert_eq!(table.columns[14].field_type, FieldType::Json);    // JSON
        assert_eq!(table.columns[15].field_type, FieldType::Uuid);    // UUID
    }

    #[test]
    fn test_parse_index() {
        let ddl = r#"
            CREATE TABLE "test" ("id" BIGINT NOT NULL, "code" VARCHAR(32), PRIMARY KEY ("id"));
            CREATE UNIQUE INDEX "uk_test_code" ON "test" ("code");
            CREATE INDEX "idx_test_id" ON "test" ("id")
        "#;
        let parser = PostgresDdlParser;
        let tables = parser.parse_ddl(ddl).unwrap();
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].indexes.len(), 2);
        assert_eq!(tables[0].indexes[0].name, "uk_test_code");
        assert_eq!(tables[0].indexes[0].kind, IndexKind::Unique);
        assert_eq!(tables[0].indexes[1].name, "idx_test_id");
        assert_eq!(tables[0].indexes[1].kind, IndexKind::Normal);
    }

    #[test]
    fn test_parse_comments() {
        let ddl = r#"
            CREATE TABLE "test" ("id" BIGINT NOT NULL, "name" VARCHAR(64), PRIMARY KEY ("id"));
            COMMENT ON TABLE "test" IS '测试表';
            COMMENT ON COLUMN "test"."id" IS '主键';
            COMMENT ON COLUMN "test"."name" IS '名称'
        "#;
        let parser = PostgresDdlParser;
        let tables = parser.parse_ddl(ddl).unwrap();
        assert_eq!(tables[0].comment, Some("测试表".to_string()));
        assert_eq!(tables[0].columns[0].label, "主键");
        assert_eq!(tables[0].columns[1].label, "名称");
    }

    #[test]
    fn test_roundtrip_ddl() {
        use crate::ddl::postgres::PostgresDdlDialect;
        use crate::ddl::DdlDialect;

        // 构造 TableDefine
        let original = TableDefine {
            table_name: "roundtrip_test".to_string(),
            display_name: "往返测试".to_string(),
            columns: vec![
                ColumnDefine {
                    name: "id".to_string(),
                    label: "主键".to_string(),
                    field_type: FieldType::Int,
                    is_primary_key: true,
                    is_nullable: false,
                    default_value: None,
                    i18n: false,
                    length: None, precision: None, scale: None,
                    db_type: None, ordinal: Some(1),
                    created_at: None, updated_at: None,
                    is_foreign_key: false,
                    foreign_key_table: None, foreign_key_column: None,
                    extensions: HashMap::new(),
                },
                ColumnDefine {
                    name: "code".to_string(),
                    label: "编码".to_string(),
                    field_type: FieldType::String,
                    is_primary_key: false,
                    is_nullable: false,
                    default_value: None,
                    i18n: false,
                    length: Some(32), precision: None, scale: None,
                    db_type: None, ordinal: Some(2),
                    created_at: None, updated_at: None,
                    is_foreign_key: false,
                    foreign_key_table: None, foreign_key_column: None,
                    extensions: HashMap::new(),
                },
                ColumnDefine {
                    name: "amount".to_string(),
                    label: "金额".to_string(),
                    field_type: FieldType::Decimal,
                    is_primary_key: false,
                    is_nullable: true,
                    default_value: Some("0".to_string()),
                    i18n: false,
                    length: None, precision: Some(10), scale: Some(2),
                    db_type: None, ordinal: Some(3),
                    created_at: None, updated_at: None,
                    is_foreign_key: false,
                    foreign_key_table: None, foreign_key_column: None,
                    extensions: HashMap::new(),
                },
            ],
            primary_keys: vec!["id".to_string()],
            indexes: vec![
                IndexDefine {
                    name: "uk_code".to_string(),
                    columns: vec!["code".to_string()],
                    kind: IndexKind::Unique,
                },
            ],
            version: 1,
            created_at: None, updated_at: None,
            i18n: false,
            comment: Some("往返测试表".to_string()),
            schema: None, tablespace: None,
            is_partitioned: false,
            partition_type: None,
            partition_columns: vec![],
            extensions: HashMap::new(),
        };

        // TableDefine → DDL
        let dialect = PostgresDdlDialect::default();
        let ddl = dialect.generate_full_ddl(&original).unwrap();

        // DDL → TableDefine
        let parser = PostgresDdlParser;
        let parsed = parser.parse_ddl(&ddl).unwrap();
        assert_eq!(parsed.len(), 1);
        let parsed = &parsed[0];

        // 验证关键字段
        assert_eq!(parsed.table_name, original.table_name);
        assert_eq!(parsed.primary_keys, original.primary_keys);
        assert_eq!(parsed.columns.len(), original.columns.len());
        assert_eq!(parsed.comment, original.comment);
        assert_eq!(parsed.indexes.len(), original.indexes.len());

        // 验证列字段
        for (orig_col, parsed_col) in original.columns.iter().zip(parsed.columns.iter()) {
            assert_eq!(orig_col.name, parsed_col.name);
            assert_eq!(orig_col.field_type, parsed_col.field_type);
            assert_eq!(orig_col.is_nullable, parsed_col.is_nullable);
            assert_eq!(orig_col.is_primary_key, parsed_col.is_primary_key);
            assert_eq!(orig_col.label, parsed_col.label);
        }
    }

    #[test]
    fn test_parse_multiple_tables() {
        let ddl = r#"
            CREATE TABLE "t1" ("id" BIGINT NOT NULL, PRIMARY KEY ("id"));
            CREATE TABLE "t2" ("id" BIGINT NOT NULL, PRIMARY KEY ("id"));
            COMMENT ON TABLE "t1" IS '表1';
            COMMENT ON TABLE "t2" IS '表2'
        "#;
        let parser = PostgresDdlParser;
        let tables = parser.parse_ddl(ddl).unwrap();
        assert_eq!(tables.len(), 2);
        assert_eq!(tables[0].table_name, "t1");
        assert_eq!(tables[0].comment, Some("表1".to_string()));
        assert_eq!(tables[1].table_name, "t2");
        assert_eq!(tables[1].comment, Some("表2".to_string()));
    }
}
