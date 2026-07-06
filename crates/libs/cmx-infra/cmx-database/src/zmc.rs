//! sqlx **PostgreSQL** 侧的 [`ZmcRowSource`] 实现 + `query_zmc` 出口。
//!
//! 与 tokio-postgres 侧(cmx-database-pg)对称:newtype 包 `sqlx::postgres::PgRow`(孤儿规则),
//! impl `ZmcRowSource`(取值转发 sqlx `try_get` + 按 `type_info().name()` 字符串分派到中立
//! `ZmcColType`)。**老的 DataSet/DataValue 路径一字不动**,这里纯新增。
//!
//! 命名 `SqlxPgRowSource` 特指 sqlx 的 PgRow;将来若加 sqlx MySQL/SQLite 支持,各自新增
//! `SqlxMySqlRowSource` / `SqlxSqliteRowSource`(按各自的 Row/TypeInfo 映射),互不影响。
//!
//! 零拷贝:sqlx `try_get::<&str>` / `try_get::<&[u8]>` 从 PgRow 的 `Bytes` 借出,不分配。

use cmx_rowsource::{ZmcColType, ZmcRowSource};
use sqlx::postgres::PgRow;
use sqlx::{Column, Row, TypeInfo};

/// sqlx PostgreSQL 一行的 newtype 包装(满足孤儿规则)。
#[repr(transparent)]
pub struct SqlxPgRowSource(pub PgRow);

impl From<PgRow> for SqlxPgRowSource {
    fn from(r: PgRow) -> Self {
        SqlxPgRowSource(r)
    }
}

impl ZmcRowSource for SqlxPgRowSource {
    fn col_count(&self) -> usize {
        self.0.columns().len()
    }

    fn col_name(&self, i: usize) -> &str {
        self.0.columns()[i].name()
    }

    fn col_type(&self, i: usize) -> ZmcColType {
        // sqlx 的 PgType 是 pub(crate),外部按 type_info().name() 字符串分派(与老 executor 一致)
        map_type_name(self.0.columns()[i].type_info().name())
    }

    fn get_bool(&self, i: usize) -> Option<bool> {
        self.0.try_get::<Option<bool>, _>(i).ok().flatten()
    }
    fn get_i16(&self, i: usize) -> Option<i16> {
        self.0.try_get::<Option<i16>, _>(i).ok().flatten()
    }
    fn get_i32(&self, i: usize) -> Option<i32> {
        self.0.try_get::<Option<i32>, _>(i).ok().flatten()
    }
    fn get_i64(&self, i: usize) -> Option<i64> {
        self.0.try_get::<Option<i64>, _>(i).ok().flatten()
    }
    fn get_f32(&self, i: usize) -> Option<f32> {
        self.0.try_get::<Option<f32>, _>(i).ok().flatten()
    }
    fn get_f64(&self, i: usize) -> Option<f64> {
        self.0.try_get::<Option<f64>, _>(i).ok().flatten()
    }
    fn get_decimal(&self, i: usize) -> Option<rust_decimal::Decimal> {
        self.0
            .try_get::<Option<rust_decimal::Decimal>, _>(i)
            .ok()
            .flatten()
    }
    fn get_str(&self, i: usize) -> Option<&str> {
        // 零拷贝借出:sqlx Decode<'r> for &'r str 从行 Bytes 借
        self.0.try_get::<Option<&str>, _>(i).ok().flatten()
    }
    fn get_bytes(&self, i: usize) -> Option<&[u8]> {
        self.0.try_get::<Option<&[u8]>, _>(i).ok().flatten()
    }
    fn get_uuid(&self, i: usize) -> Option<uuid::Uuid> {
        self.0.try_get::<Option<uuid::Uuid>, _>(i).ok().flatten()
    }
    fn get_date(&self, i: usize) -> Option<chrono::NaiveDate> {
        self.0
            .try_get::<Option<chrono::NaiveDate>, _>(i)
            .ok()
            .flatten()
    }
    fn get_naive_datetime(&self, i: usize) -> Option<chrono::NaiveDateTime> {
        self.0
            .try_get::<Option<chrono::NaiveDateTime>, _>(i)
            .ok()
            .flatten()
    }
    fn get_datetime_utc(&self, i: usize) -> Option<chrono::DateTime<chrono::Utc>> {
        self.0
            .try_get::<Option<chrono::DateTime<chrono::Utc>>, _>(i)
            .ok()
            .flatten()
    }
    fn get_json_value(&self, i: usize) -> Option<serde_json::Value> {
        self.0
            .try_get::<Option<serde_json::Value>, _>(i)
            .ok()
            .flatten()
    }
}

/// sqlx 列类型名(`type_info().name()`)→ 中立 `ZmcColType`。
fn map_type_name(name: &str) -> ZmcColType {
    // sqlx 的 name() 返回大写(如 "INT4"),稳妥起见转小写匹配
    match name.to_ascii_lowercase().as_str() {
        "bool" => ZmcColType::Bool,
        "int2" | "smallint" | "smallserial" => ZmcColType::Int2,
        "int4" | "int" | "serial" => ZmcColType::Int4,
        "int8" | "bigint" | "bigserial" => ZmcColType::Int8,
        "float4" | "real" => ZmcColType::Float4,
        "float8" | "double precision" => ZmcColType::Float8,
        "numeric" => ZmcColType::Numeric,
        "text" | "varchar" | "bpchar" | "name" | "char" | "\"char\"" => ZmcColType::Text,
        "json" => ZmcColType::Json,
        "jsonb" => ZmcColType::Jsonb,
        "uuid" => ZmcColType::Uuid,
        "bytea" => ZmcColType::Bytea,
        "date" => ZmcColType::Date,
        "timestamp" => ZmcColType::Timestamp,
        "timestamptz" => ZmcColType::Timestamptz,
        _ => ZmcColType::Unknown,
    }
}

/// 对外类型别名:sqlx 侧 ZmcDataSet。
pub type ZmcDataSet = cmx_rowsource::ZmcDataSet<SqlxPgRowSource>;

/// 对外类型别名:sqlx 侧 ZmcChildGroup(子集分组,由装载器组装、编码器按父 id 分桶)。
pub type ZmcChildGroup = cmx_rowsource::ZmcChildGroup<SqlxPgRowSource>;

/// 中立 schema 重导出:与 tokio 侧(cmx-database-pg)对称,让 sqlx 侧装载器同名取用。
pub use cmx_rowsource::ZmcSchema;
