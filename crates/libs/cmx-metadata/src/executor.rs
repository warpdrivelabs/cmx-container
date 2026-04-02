//! DDL 执行模块
//!
//! 提供 DDL 执行函数和 `PgTableDefineExecutor` 实现。
//! 依赖 cmx-database 提供的底层 SQL 执行能力。
//!
//! # 功能特性
//! - 支持执行单条和多条 DDL 语句
//! - 提供 `TableDefineDbExecutor` trait 的 PostgreSQL 实现
//! - 自动生成 CREATE TABLE、COMMENT、INDEX 语句
//! - 支持 upgrade_table：查询当前表结构，diff 后执行增量 DDL

use std::collections::HashMap;

use async_trait::async_trait;
use cmx_core::model::cell::{
    ColumnDefine, DataValue, FieldType, IndexDefine, IndexKind, TableDefine,
};
use cmx_database::{DataSet, get_default_db_manager};
use thiserror::Error;
use tracing::info;

// ==========================================
// 错误类型
// ==========================================

/// 基础错误类型
#[derive(Error, Debug)]
pub enum BaseError {
    /// IO 错误
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
    /// JSON 解析错误
    #[error("JSON 解析错误: {0}")]
    Json(#[from] serde_json::Error),
    /// 未实现错误
    #[error("未实现: {0}")]
    Unimplemented(String),
    /// 配置未找到错误
    #[error("配置未找到: {0}")]
    ConfigNotFound(String),
    /// 配置依赖错误
    #[error("配置依赖错误: {0}")]
    ConfigDependency(String),
    /// DDL 生成错误
    #[error("DDL 生成错误: {0}")]
    DdlGeneration(String),
    /// DDL 解析错误
    #[error("DDL 解析错误: {0}")]
    DdlParse(String),
}

// ==========================================
// 数据库创建/升级表接口
// ==========================================

/// 表定义在数据库中执行"创建或升级"的接口
///
/// 后续由具体数据库实现（如 PostgreSQL / MySQL）。
/// 使用 #[async_trait] 支持异步操作。
#[async_trait]
pub trait TableDefineDbExecutor: Send + Sync {
    /// 创建表
    ///
    /// # 参数
    /// - `define`: 表定义
    async fn create_table(&self, define: &TableDefine) -> Result<(), BaseError>;

    /// 升级表
    ///
    /// # 参数
    /// - `define`: 表定义
    async fn upgrade_table(&self, define: &TableDefine) -> Result<(), BaseError>;

    /// 创建或升级表
    ///
    /// 默认实现：先尝试创建，失败则尝试升级
    ///
    /// # 参数
    /// - `define`: 表定义
    async fn create_or_upgrade_table(&self, define: &TableDefine) -> Result<(), BaseError> {
        if self.create_table(define).await.is_ok() {
            return Ok(());
        }
        self.upgrade_table(define).await
    }
}

use crate::MetadataError;
use crate::ddl::DdlDialect;
use crate::ddl::diff::DdlDiff;
use crate::ddl::postgres::PostgresDdlDialect;

/// 执行多条 DDL 语句（逐条执行）
pub async fn execute_ddl_by_ids(
    db_id: &str,
    txn_id: Option<&str>,
    statements: &[String],
) -> Result<(), MetadataError> {
    for stmt in statements {
        get_default_db_manager()
            .execute_sql(db_id, txn_id, stmt)
            .await
            .map_err(|e| MetadataError::DdlExecution(e.to_string()))?;
    }
    Ok(())
}

/// 执行单条 DDL 语句
pub async fn execute_ddl_statement_by_ids(
    db_id: &str,
    txn_id: Option<&str>,
    statement: &str,
) -> Result<u64, MetadataError> {
    get_default_db_manager()
        .execute_sql(db_id, txn_id, statement)
        .await
        .map_err(|e| MetadataError::DdlExecution(e.to_string()))
}

/// PostgreSQL TableDefine 执行器
///
/// 将 TableDefine 转为 DDL 语句后通过数据库连接执行。
pub struct PgTableDefineExecutor {
    pub db_id: String,
    pub txn_id: Option<String>,
}

impl PgTableDefineExecutor {
    pub fn new(db_id: impl Into<String>, txn_id: Option<String>) -> Self {
        Self {
            db_id: db_id.into(),
            txn_id,
        }
    }

    /// 执行多条 DDL 语句（内部辅助）
    async fn execute_statements(&self, statements: Vec<String>) -> Result<(), MetadataError> {
        let db_id = self.db_id.clone();
        let txn_id = self.txn_id.clone();
        for stmt in &statements {
            get_default_db_manager()
                .execute_sql(&db_id, txn_id.as_deref(), stmt)
                .await
                .map_err(|e| MetadataError::DdlExecution(e.to_string()))?;
        }
        Ok(())
    }

    /// 查询 PostgreSQL 中当前表的结构，构建 TableDefine
    ///
    /// 通过查询 information_schema.columns、pg_indexes、pg_description
    /// 还原当前数据库中的表定义，用于与新定义进行 diff。
    async fn query_current_table_define(
        &self,
        define: &TableDefine,
    ) -> Result<TableDefine, MetadataError> {
        let db_id = self.db_id.clone();
        let txn_id = self.txn_id.clone();
        let table_name = define.table_name.clone();
        let schema_name = define
            .schema
            .clone()
            .unwrap_or_else(|| "public".to_string());

        // 1. 查询列信息
        let columns_sql = format!(
            "SELECT column_name, data_type, character_maximum_length, \
                 numeric_precision, numeric_scale, is_nullable, column_default, \
                 ordinal_position, udt_name \
                 FROM information_schema.columns \
                 WHERE table_schema = '{}' AND table_name = '{}' \
                 ORDER BY ordinal_position",
            schema_name, table_name
        );
        let col_ds = get_default_db_manager()
            .query_sql(&db_id, txn_id.as_deref(), &columns_sql, "columns")
            .await
            .map_err(|e| MetadataError::DdlExecution(format!("查询列信息失败: {}", e)))?;

        if col_ds.is_empty() {
            return Err(MetadataError::DdlExecution(format!(
                "表 {}.{} 不存在或无列信息",
                schema_name, table_name
            )));
        }

        // 2. 查询主键列
        let pk_sql = format!(
            "SELECT a.attname \
                 FROM pg_index i \
                 JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = ANY(i.indkey) \
                 WHERE i.indrelid = '{}.{}'::regclass AND i.indisprimary \
                 ORDER BY array_position(i.indkey, a.attnum)",
            schema_name, table_name
        );
        let pk_ds = get_default_db_manager()
            .query_sql(&db_id, txn_id.as_deref(), &pk_sql, "pk")
            .await
            .unwrap_or_else(|_| empty_dataset("pk"));

        // 3. 查询索引信息（排除主键索引）
        let idx_sql = format!(
            "SELECT i.relname AS index_name, \
                 ix.indisunique AS is_unique, \
                 array_agg(a.attname ORDER BY array_position(ix.indkey, a.attnum)) AS columns \
                 FROM pg_class t \
                 JOIN pg_index ix ON t.oid = ix.indrelid \
                 JOIN pg_class i ON i.oid = ix.indexrelid \
                 JOIN pg_attribute a ON a.attrelid = t.oid AND a.attnum = ANY(ix.indkey) \
                 JOIN pg_namespace n ON n.oid = t.relnamespace \
                 WHERE t.relname = '{}' AND n.nspname = '{}' \
                 AND NOT ix.indisprimary \
                 GROUP BY i.relname, ix.indisunique \
                 ORDER BY i.relname",
            table_name, schema_name
        );
        let idx_ds = get_default_db_manager()
            .query_sql(&db_id, txn_id.as_deref(), &idx_sql, "indexes")
            .await
            .unwrap_or_else(|_| empty_dataset("indexes"));

        // 4. 查询表注释
        let comment_sql = format!(
            "SELECT obj_description('{}.{}'::regclass, 'pg_class') AS comment",
            schema_name, table_name
        );
        let comment_ds = get_default_db_manager()
            .query_sql(&db_id, txn_id.as_deref(), &comment_sql, "comment")
            .await
            .unwrap_or_else(|_| empty_dataset("comment"));

        // 5. 查询列注释
        let col_comment_sql = format!(
            "SELECT a.attname AS column_name, \
                 col_description(a.attrelid, a.attnum) AS comment \
                 FROM pg_attribute a \
                 JOIN pg_class c ON a.attrelid = c.oid \
                 JOIN pg_namespace n ON c.relnamespace = n.oid \
                 WHERE c.relname = '{}' AND n.nspname = '{}' \
                 AND a.attnum > 0 AND NOT a.attisdropped \
                 AND col_description(a.attrelid, a.attnum) IS NOT NULL",
            table_name, schema_name
        );
        let col_comment_ds = get_default_db_manager()
            .query_sql(&db_id, txn_id.as_deref(), &col_comment_sql, "col_comments")
            .await
            .unwrap_or_else(|_| empty_dataset("col_comments"));

        // 构建列注释映射
        let mut col_comment_map: HashMap<String, String> = HashMap::new();
        for row in col_comment_ds.iter() {
            let col_name = get_string_field(row.get(0));
            let comment = get_string_field(row.get(1));
            if !col_name.is_empty() && !comment.is_empty() {
                col_comment_map.insert(col_name, comment);
            }
        }

        // 构建主键列集合
        let mut primary_keys: Vec<String> = Vec::new();
        for row in pk_ds.iter() {
            let pk_col = get_string_field(row.get(0));
            if !pk_col.is_empty() {
                primary_keys.push(pk_col);
            }
        }

        // 构建列定义
        let mut columns: Vec<ColumnDefine> = Vec::new();
        for row in col_ds.iter() {
            let name = get_string_field(row.get(0));
            let data_type = get_string_field(row.get(1));
            let char_max_len = get_opt_i64(row.get(2));
            let num_precision = get_opt_i64(row.get(3));
            let num_scale = get_opt_i64(row.get(4));
            let is_nullable_str = get_string_field(row.get(5));
            let column_default = get_opt_string(row.get(6));
            let ordinal = get_opt_i64(row.get(7));
            let udt_name = get_string_field(row.get(8));

            let is_nullable = is_nullable_str == "YES";
            let is_pk = primary_keys.contains(&name);

            // 构建原始 db_type
            let db_type = build_pg_db_type(
                &data_type,
                &udt_name,
                char_max_len,
                num_precision,
                num_scale,
            );

            // 映射为 FieldType
            let field_type = pg_type_to_field_type(&data_type, &udt_name);

            // 处理默认值（去掉类型转换后缀）
            let default_value = column_default.map(|d| clean_pg_default(&d));

            let label = col_comment_map.get(&name).cloned().unwrap_or_default();

            columns.push(ColumnDefine {
                name,
                label,
                field_type,
                is_primary_key: is_pk,
                is_nullable,
                default_value,
                i18n: false,
                length: char_max_len.map(|v| v as u32),
                precision: num_precision.map(|v| v as u32),
                scale: num_scale.map(|v| v as u32),
                db_type: Some(db_type),
                ordinal: ordinal.map(|v| v as u32),
                create_time: None,
                update_time: None,
                is_foreign_key: false,
                foreign_key_table: None,
                foreign_key_column: None,
                extensions: HashMap::new(),
            });
        }

        // 构建索引定义
        let mut indexes: Vec<IndexDefine> = Vec::new();
        for row in idx_ds.iter() {
            let idx_name = get_string_field(row.get(0));
            let is_unique = match row.get(1) {
                Some(DataValue::Bool(b)) => *b,
                _ => false,
            };
            // columns 字段可能是 String（PostgreSQL array_agg 返回的文本格式）
            let idx_columns = parse_pg_array_column(row.get(2));

            if !idx_name.is_empty() {
                indexes.push(IndexDefine {
                    name: idx_name,
                    columns: idx_columns,
                    kind: if is_unique {
                        IndexKind::Unique
                    } else {
                        IndexKind::Normal
                    },
                });
            }
        }

        // 表注释
        let table_comment = if !comment_ds.is_empty() {
            get_opt_string(comment_ds.iter().next().and_then(|r| r.get(0)))
        } else {
            None
        };

        Ok(TableDefine {
            table_name: table_name.clone(),
            display_name: table_comment.clone().unwrap_or_else(|| table_name.clone()),
            columns,
            primary_keys,
            indexes,
            version: define.version,
            create_time: None,
            update_time: None,
            i18n: false,
            comment: table_comment,
            schema: Some(schema_name),
            tablespace: None,
            is_partitioned: false,
            partition_type: None,
            partition_columns: vec![],
            extensions: HashMap::new(),
        })
    }
}

#[async_trait]
impl TableDefineDbExecutor for PgTableDefineExecutor {
    async fn create_table(&self, define: &TableDefine) -> std::result::Result<(), BaseError> {
        let dialect = PostgresDdlDialect::default();

        // 生成 CREATE TABLE + COMMENT + INDEX
        let create_stmt = dialect
            .generate_create_table(define)
            .map_err(|e| BaseError::DdlGeneration(e.to_string()))?;
        let comment_stmts = dialect
            .generate_comments(define)
            .map_err(|e| BaseError::DdlGeneration(e.to_string()))?;
        let index_stmts = dialect
            .generate_create_indexes(define)
            .map_err(|e| BaseError::DdlGeneration(e.to_string()))?;
        info!("PgTableDefineExecutor::create_stmt: {:?}", create_stmt);
        info!("PgTableDefineExecutor::comment_stmts: {:?}", comment_stmts);
        info!("PgTableDefineExecutor::index_stmts: {:?}", index_stmts);

        let mut all_stmts = vec![create_stmt];
        all_stmts.extend(comment_stmts);
        all_stmts.extend(index_stmts);

        self.execute_statements(all_stmts)
            .await
            .map_err(|e| BaseError::DdlGeneration(e.to_string()))
    }

    async fn upgrade_table(&self, define: &TableDefine) -> std::result::Result<(), BaseError> {
        // 1. 查询当前数据库中的表结构
        let current = self
            .query_current_table_define(define)
            .await
            .map_err(|e| BaseError::DdlGeneration(format!("查询当前表结构失败: {}", e)))?;

        info!(
            "upgrade_table: 当前表 {} 有 {} 列, {} 索引",
            current.table_name,
            current.columns.len(),
            current.indexes.len()
        );

        // 2. 使用 DdlDiff 生成增量 DDL
        let dialect = PostgresDdlDialect {
            prefer_db_type: true,
        };
        let stmts = DdlDiff::diff_to_ddl(&dialect, &[current], &[define.clone()])
            .map_err(|e| BaseError::DdlGeneration(format!("生成增量 DDL 失败: {}", e)))?;

        if stmts.is_empty() {
            info!("upgrade_table: 表 {} 无变更", define.table_name);
            return Ok(());
        }

        info!(
            "upgrade_table: 表 {} 生成 {} 条增量 DDL",
            define.table_name,
            stmts.len()
        );
        for stmt in &stmts {
            info!("  DDL: {}", stmt);
        }

        // 3. 执行增量 DDL
        self.execute_statements(stmts)
            .await
            .map_err(|e| BaseError::DdlGeneration(format!("执行增量 DDL 失败: {}", e)))
    }
}

// ==========================================
// 辅助函数
// ==========================================

/// 从 DataValue 中提取字符串值
fn get_string_field(val: Option<&DataValue>) -> String {
    match val {
        Some(DataValue::String(s)) => s.clone(),
        Some(DataValue::ShortStr(s)) => s.to_string(),
        Some(DataValue::LongStr(s)) => s.to_string(),
        _ => String::new(),
    }
}

/// 从 DataValue 中提取可选字符串
fn get_opt_string(val: Option<&DataValue>) -> Option<String> {
    match val {
        Some(DataValue::String(s)) if !s.is_empty() => Some(s.clone()),
        Some(DataValue::ShortStr(s)) if !s.is_empty() => Some(s.to_string()),
        Some(DataValue::LongStr(s)) if !s.is_empty() => Some(s.to_string()),
        _ => None,
    }
}

/// 从 DataValue 中提取可选 i64
fn get_opt_i64(val: Option<&DataValue>) -> Option<i64> {
    match val {
        Some(DataValue::Int(i)) => Some(*i),
        Some(DataValue::String(s)) => s.parse().ok(),
        _ => None,
    }
}

/// 构建 PostgreSQL 数据库类型字符串（如 VARCHAR(32)、NUMERIC(10,2)）
fn build_pg_db_type(
    data_type: &str,
    udt_name: &str,
    char_max_len: Option<i64>,
    num_precision: Option<i64>,
    num_scale: Option<i64>,
) -> String {
    let upper = data_type.to_uppercase();
    match upper.as_str() {
        "CHARACTER VARYING" | "character varying" => {
            if let Some(len) = char_max_len {
                format!("VARCHAR({})", len)
            } else {
                "VARCHAR".to_string()
            }
        }
        "CHARACTER" | "character" => {
            if let Some(len) = char_max_len {
                format!("CHAR({})", len)
            } else {
                "CHAR".to_string()
            }
        }
        "NUMERIC" | "numeric" => match (num_precision, num_scale) {
            (Some(p), Some(s)) if s > 0 => format!("NUMERIC({},{})", p, s),
            (Some(p), _) => format!("NUMERIC({})", p),
            _ => "NUMERIC".to_string(),
        },
        _ => {
            // 对于其他类型，优先使用 udt_name 的大写形式
            let name = if udt_name.is_empty() {
                data_type
            } else {
                udt_name
            };
            pg_udt_to_display(name)
        }
    }
}

/// 将 PostgreSQL udt_name 映射为常规显示类型
fn pg_udt_to_display(udt: &str) -> String {
    match udt {
        "int2" => "SMALLINT".to_string(),
        "int4" => "INTEGER".to_string(),
        "int8" => "BIGINT".to_string(),
        "float4" => "REAL".to_string(),
        "float8" => "DOUBLE PRECISION".to_string(),
        "bool" => "BOOLEAN".to_string(),
        "text" => "TEXT".to_string(),
        "varchar" => "VARCHAR".to_string(),
        "bpchar" => "CHAR".to_string(),
        "timestamp" => "TIMESTAMP".to_string(),
        "timestamptz" => "TIMESTAMP WITH TIME ZONE".to_string(),
        "date" => "DATE".to_string(),
        "time" => "TIME".to_string(),
        "timetz" => "TIME WITH TIME ZONE".to_string(),
        "uuid" => "UUID".to_string(),
        "json" => "JSON".to_string(),
        "jsonb" => "JSONB".to_string(),
        "bytea" => "BYTEA".to_string(),
        "numeric" => "NUMERIC".to_string(),
        _ => udt.to_uppercase(),
    }
}

/// 将 PostgreSQL 类型映射为 FieldType
fn pg_type_to_field_type(data_type: &str, udt_name: &str) -> FieldType {
    let dt = data_type.to_lowercase();
    let udt = udt_name.to_lowercase();

    match dt.as_str() {
        "integer" | "bigint" | "smallint" => FieldType::Int,
        "real" | "double precision" => FieldType::Float,
        "numeric" | "decimal" => FieldType::Decimal,
        "boolean" => FieldType::Bool,
        "text" => FieldType::Text,
        "character varying" | "character" => FieldType::String,
        "date" => FieldType::Date,
        "timestamp with time zone" | "timestamp without time zone" => FieldType::DateTime,
        "uuid" => FieldType::Uuid,
        "json" | "jsonb" => FieldType::Json,
        "bytea" => FieldType::Binary,
        "array" | "ARRAY" => FieldType::Json,
        _ => {
            // 按 udt_name 再匹配一次
            match udt.as_str() {
                "int2" | "int4" | "int8" => FieldType::Int,
                "float4" | "float8" => FieldType::Float,
                "bool" => FieldType::Bool,
                "text" => FieldType::Text,
                "varchar" | "bpchar" => FieldType::String,
                "date" => FieldType::Date,
                "timestamp" | "timestamptz" => FieldType::DateTime,
                "uuid" => FieldType::Uuid,
                "json" | "jsonb" => FieldType::Json,
                "bytea" => FieldType::Binary,
                "numeric" => FieldType::Decimal,
                _ => FieldType::String,
            }
        }
    }
}

/// 清理 PostgreSQL 默认值（去掉类型转换后缀、序列引用等）
fn clean_pg_default(raw: &str) -> String {
    let s = raw.trim();
    // 去掉序列引用（如 nextval('xxx'::regclass)）
    if s.starts_with("nextval(") {
        return String::new();
    }
    // 去掉类型转换后缀（如 'active'::character varying → active）
    if let Some(pos) = s.find("::") {
        let before = s[..pos].trim();
        // 去掉单引号
        return before.trim_matches('\'').to_string();
    }
    // 去掉 NULL
    if s.eq_ignore_ascii_case("NULL") {
        return String::new();
    }
    s.to_string()
}

/// 解析 PostgreSQL array_agg 返回的列名数组
/// 可能是 DataValue::String("{col1,col2}") 或 DataValue::Array
fn parse_pg_array_column(val: Option<&DataValue>) -> Vec<String> {
    match val {
        Some(DataValue::String(s)) => {
            // PostgreSQL text 数组格式: {col1,col2,col3}
            let trimmed = s.trim_matches(|c| c == '{' || c == '}');
            if trimmed.is_empty() {
                vec![]
            } else {
                trimmed
                    .split(',')
                    .map(|s| s.trim().trim_matches('"').to_string())
                    .collect()
            }
        }
        Some(DataValue::Array(arr)) => arr
            .iter()
            .filter_map(|v| match v {
                DataValue::String(s) => Some(s.clone()),
                DataValue::ShortStr(s) => Some(s.to_string()),
                _ => None,
            })
            .collect(),
        _ => vec![],
    }
}

/// 创建空 DataSet（查询失败时的 fallback）
fn empty_dataset(id: &str) -> DataSet {
    use cmx_core::model::data::dataset::Schema;
    use std::sync::Arc;
    DataSet::empty(id, Arc::new(Schema::new(id, vec![])))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pg_type_to_field_type() {
        assert_eq!(pg_type_to_field_type("integer", "int4"), FieldType::Int);
        assert_eq!(pg_type_to_field_type("bigint", "int8"), FieldType::Int);
        assert_eq!(
            pg_type_to_field_type("character varying", "varchar"),
            FieldType::String
        );
        assert_eq!(pg_type_to_field_type("text", "text"), FieldType::Text);
        assert_eq!(pg_type_to_field_type("boolean", "bool"), FieldType::Bool);
        assert_eq!(
            pg_type_to_field_type("numeric", "numeric"),
            FieldType::Decimal
        );
        assert_eq!(
            pg_type_to_field_type("timestamp with time zone", "timestamptz"),
            FieldType::DateTime
        );
        assert_eq!(pg_type_to_field_type("date", "date"), FieldType::Date);
        assert_eq!(pg_type_to_field_type("uuid", "uuid"), FieldType::Uuid);
        assert_eq!(pg_type_to_field_type("json", "json"), FieldType::Json);
        assert_eq!(pg_type_to_field_type("bytea", "bytea"), FieldType::Binary);
    }

    #[test]
    fn test_build_pg_db_type() {
        assert_eq!(
            build_pg_db_type("character varying", "varchar", Some(32), None, None),
            "VARCHAR(32)"
        );
        assert_eq!(
            build_pg_db_type("character varying", "varchar", None, None, None),
            "VARCHAR"
        );
        assert_eq!(
            build_pg_db_type("numeric", "numeric", None, Some(10), Some(2)),
            "NUMERIC(10,2)"
        );
        assert_eq!(
            build_pg_db_type("integer", "int4", None, Some(32), Some(0)),
            "INTEGER"
        );
        assert_eq!(
            build_pg_db_type("bigint", "int8", None, Some(64), Some(0)),
            "BIGINT"
        );
        assert_eq!(build_pg_db_type("text", "text", None, None, None), "TEXT");
        assert_eq!(
            build_pg_db_type("timestamp with time zone", "timestamptz", None, None, None),
            "TIMESTAMP WITH TIME ZONE"
        );
    }

    #[test]
    fn test_clean_pg_default() {
        assert_eq!(clean_pg_default("0"), "0");
        assert_eq!(clean_pg_default("'active'::character varying"), "active");
        assert_eq!(clean_pg_default("nextval('seq'::regclass)"), "");
        assert_eq!(clean_pg_default("NULL"), "");
        assert_eq!(clean_pg_default("true"), "true");
        assert_eq!(clean_pg_default("100"), "100");
    }

    #[test]
    fn test_parse_pg_array_column() {
        assert_eq!(
            parse_pg_array_column(Some(&DataValue::String("{col1,col2,col3}".to_string()))),
            vec!["col1", "col2", "col3"]
        );
        assert_eq!(
            parse_pg_array_column(Some(&DataValue::String("{}".to_string()))),
            Vec::<String>::new()
        );
        assert_eq!(parse_pg_array_column(None), Vec::<String>::new());
    }
}
