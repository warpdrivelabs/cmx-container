//! tokio-postgres 侧的 [`ZmcRowSource`] 实现。
//!
//! ZmcDataSet / ZmcSchema / 编码器等已上移到 `cmx-rowsource`(驱动无关)。这里只保留:
//! - newtype `TokioPgRowSource(tokio_postgres::Row)` + `impl ZmcRowSource`(孤儿规则:trait 和 Row
//!   都是外部类型,必须包一层本地 newtype)
//! - 类型别名 `ZmcDataSet = cmx_rowsource::ZmcDataSet<TokioPgRowSource>` 等,保持对外 API 名不变

use cmx_rowsource::{ZmcColType, ZmcRowSource};
use tokio_postgres::Row;
use tokio_postgres::types::Type;

/// tokio-postgres 一行的 newtype 包装(满足孤儿规则)。
///
/// 透明包装,`Vec<TokioPgRowSource>` 与 `Vec<Row>` 内存布局一致(newtype 零开销)。
#[repr(transparent)]
pub struct TokioPgRowSource(pub Row);

impl From<Row> for TokioPgRowSource {
    fn from(r: Row) -> Self {
        TokioPgRowSource(r)
    }
}

impl ZmcRowSource for TokioPgRowSource {
    fn col_count(&self) -> usize {
        self.0.columns().len()
    }

    fn col_name(&self, i: usize) -> &str {
        self.0.columns()[i].name()
    }

    fn col_type(&self, i: usize) -> ZmcColType {
        map_type(self.0.columns()[i].type_())
    }

    fn get_bool(&self, i: usize) -> Option<bool> {
        self.0.try_get::<usize, Option<bool>>(i).ok().flatten()
    }
    fn get_i16(&self, i: usize) -> Option<i16> {
        self.0.try_get::<usize, Option<i16>>(i).ok().flatten()
    }
    fn get_i32(&self, i: usize) -> Option<i32> {
        self.0.try_get::<usize, Option<i32>>(i).ok().flatten()
    }
    fn get_i64(&self, i: usize) -> Option<i64> {
        self.0.try_get::<usize, Option<i64>>(i).ok().flatten()
    }
    fn get_f32(&self, i: usize) -> Option<f32> {
        self.0.try_get::<usize, Option<f32>>(i).ok().flatten()
    }
    fn get_f64(&self, i: usize) -> Option<f64> {
        self.0.try_get::<usize, Option<f64>>(i).ok().flatten()
    }
    fn get_decimal(&self, i: usize) -> Option<rust_decimal::Decimal> {
        self.0
            .try_get::<usize, Option<rust_decimal::Decimal>>(i)
            .ok()
            .flatten()
    }
    fn get_str(&self, i: usize) -> Option<&str> {
        self.0.try_get::<usize, Option<&str>>(i).ok().flatten()
    }
    fn get_bytes(&self, i: usize) -> Option<&[u8]> {
        self.0.try_get::<usize, Option<&[u8]>>(i).ok().flatten()
    }
    fn get_uuid(&self, i: usize) -> Option<uuid::Uuid> {
        self.0.try_get::<usize, Option<uuid::Uuid>>(i).ok().flatten()
    }
    fn get_date(&self, i: usize) -> Option<chrono::NaiveDate> {
        self.0
            .try_get::<usize, Option<chrono::NaiveDate>>(i)
            .ok()
            .flatten()
    }
    fn get_naive_datetime(&self, i: usize) -> Option<chrono::NaiveDateTime> {
        self.0
            .try_get::<usize, Option<chrono::NaiveDateTime>>(i)
            .ok()
            .flatten()
    }
    fn get_datetime_utc(&self, i: usize) -> Option<chrono::DateTime<chrono::Utc>> {
        self.0
            .try_get::<usize, Option<chrono::DateTime<chrono::Utc>>>(i)
            .ok()
            .flatten()
    }
    fn get_json_value(&self, i: usize) -> Option<serde_json::Value> {
        self.0
            .try_get::<usize, Option<serde_json::Value>>(i)
            .ok()
            .flatten()
    }
}

/// `postgres_types::Type` → 中立 `ZmcColType`。
fn map_type(ty: &Type) -> ZmcColType {
    match *ty {
        Type::BOOL => ZmcColType::Bool,
        Type::INT2 => ZmcColType::Int2,
        Type::INT4 => ZmcColType::Int4,
        Type::INT8 => ZmcColType::Int8,
        Type::FLOAT4 => ZmcColType::Float4,
        Type::FLOAT8 => ZmcColType::Float8,
        Type::NUMERIC => ZmcColType::Numeric,
        Type::TEXT | Type::VARCHAR | Type::BPCHAR | Type::NAME | Type::CHAR => ZmcColType::Text,
        Type::JSON => ZmcColType::Json,
        Type::JSONB => ZmcColType::Jsonb,
        Type::UUID => ZmcColType::Uuid,
        Type::BYTEA => ZmcColType::Bytea,
        Type::DATE => ZmcColType::Date,
        Type::TIMESTAMP => ZmcColType::Timestamp,
        Type::TIMESTAMPTZ => ZmcColType::Timestamptz,
        _ => ZmcColType::Unknown,
    }
}

/// 对外类型别名:保持 `cmx_database_pg::ZmcDataSet` 等 API 名不变。
pub type ZmcDataSet = cmx_rowsource::ZmcDataSet<TokioPgRowSource>;
pub type ZmcChildGroup = cmx_rowsource::ZmcChildGroup<TokioPgRowSource>;
pub use cmx_rowsource::ZmcSchema;
pub use cmx_rowsource::{encode_row_into, encode_stream_footer, encode_stream_header};
