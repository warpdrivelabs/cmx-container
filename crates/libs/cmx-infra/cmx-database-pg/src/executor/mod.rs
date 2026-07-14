//! 执行层（tokio-postgres 版）
//!
//! 三块核心逻辑，与 cmx-database（sqlx 版）的 executor 对齐：
//! 1. `PgResultConverter`：tokio-postgres `Row` → `DataSet`，按 `postgres_types::Type`
//!    的 OID 精确分派（**不能像 sqlx 那样靠字符串 contains + 逐类型 try**，因为
//!    tokio-postgres 的 `Row::get::<T>` 类型不符会 **panic**；一律走
//!    `try_get::<Option<T>>` + Type 匹配，任何失败降级 `DataValue::Null`）。
//! 2. `bind_data_values_pg`：`DataValue` → `Vec<Box<dyn ToSql + Sync + Send>>`（手写 SQL 路径）。
//! 3. `sea_values_to_tosql`：`sea_query::Values` → `Vec<Box<dyn ToSql + Sync + Send>>`（crud 路径）。

use cmx_core::model::cell::{DataValue, Field, FieldType, SqlTypeMarker};
use cmx_core::model::data::dataset::{DataSet, Row, Schema};
use postgres_types::{IsNull, ToSql, Type};
use rust_decimal::Decimal;
use tokio_postgres::Row as PgRow;
use uuid::Uuid;

/// 宽度自适应整数参数包装。
///
/// cmx 的 `DataValue::Int` 统一是 `i64`，但 tokio-postgres 的 `ToSql` 严格按列类型校验：
/// 把 `i64` 绑到 `INT4`/`INT2` 列会报 `WrongType`（sqlx 会隐式协调，tokio-postgres 不会）。
/// `PgInt` 的 `accepts` 允许 INT2/INT4/INT8，`to_sql` 按目标列宽度下转并委托对应整型的
/// `ToSql`（超出目标范围时报错，避免静默截断）。
#[derive(Debug, Clone, Copy)]
struct PgInt(i64);

impl ToSql for PgInt {
    fn to_sql(
        &self,
        ty: &Type,
        out: &mut bytes::BytesMut,
    ) -> Result<IsNull, Box<dyn std::error::Error + Sync + Send>> {
        match *ty {
            Type::INT2 => {
                let v = i16::try_from(self.0)
                    .map_err(|_| format!("值 {} 超出 INT2 范围", self.0))?;
                v.to_sql(ty, out)
            }
            Type::INT4 => {
                let v = i32::try_from(self.0)
                    .map_err(|_| format!("值 {} 超出 INT4 范围", self.0))?;
                v.to_sql(ty, out)
            }
            // INT8 及其它默认按 i64
            _ => self.0.to_sql(ty, out),
        }
    }

    fn accepts(ty: &Type) -> bool {
        matches!(*ty, Type::INT2 | Type::INT4 | Type::INT8)
    }

    postgres_types::to_sql_checked!();
}

/// 时区自适应的时间戳参数。
///
/// cmx 的 `DataValue::DateTime` 统一是 `DateTime<Utc>`（映射 TIMESTAMPTZ），但列可能是
/// `TIMESTAMP`（无时区）。`DateTime<Utc>` 无法直接绑到 `TIMESTAMP` 列。`PgDateTime` 的
/// `accepts` 同时允许 TIMESTAMP / TIMESTAMPTZ：绑 TIMESTAMP 时取其 naive（UTC 墙钟）部分。
#[derive(Debug, Clone, Copy)]
struct PgDateTime(chrono::DateTime<chrono::Utc>);

impl ToSql for PgDateTime {
    fn to_sql(
        &self,
        ty: &Type,
        out: &mut bytes::BytesMut,
    ) -> Result<IsNull, Box<dyn std::error::Error + Sync + Send>> {
        match *ty {
            Type::TIMESTAMP => self.0.naive_utc().to_sql(ty, out),
            _ => self.0.to_sql(ty, out),
        }
    }

    fn accepts(ty: &Type) -> bool {
        matches!(*ty, Type::TIMESTAMP | Type::TIMESTAMPTZ)
    }

    postgres_types::to_sql_checked!();
}

/// 时区自适应的时间戳 NULL 参数（同 [`PgDateTime`]，恒 NULL）。
#[derive(Debug, Clone, Copy)]
struct PgDateTimeNull;

impl ToSql for PgDateTimeNull {
    fn to_sql(
        &self,
        _ty: &Type,
        _out: &mut bytes::BytesMut,
    ) -> Result<IsNull, Box<dyn std::error::Error + Sync + Send>> {
        Ok(IsNull::Yes)
    }

    fn accepts(ty: &Type) -> bool {
        matches!(*ty, Type::TIMESTAMP | Type::TIMESTAMPTZ)
    }

    postgres_types::to_sql_checked!();
}
///
/// `NullTyped(Int)` 需要绑一个能匹配 INT2/INT4/INT8 任意列的 NULL；`Option::<i64>::None`
/// 只 `accepts` INT8，绑到 INT4 列会 `WrongType`。`PgIntNull` 的 `accepts` 放宽到三种整型，
/// `to_sql` 恒返回 `IsNull::Yes`。
#[derive(Debug, Clone, Copy)]
struct PgIntNull;

impl ToSql for PgIntNull {
    fn to_sql(
        &self,
        _ty: &Type,
        _out: &mut bytes::BytesMut,
    ) -> Result<IsNull, Box<dyn std::error::Error + Sync + Send>> {
        Ok(IsNull::Yes)
    }

    fn accepts(ty: &Type) -> bool {
        matches!(*ty, Type::INT2 | Type::INT4 | Type::INT8)
    }

    postgres_types::to_sql_checked!();
}

/// 参数值类型（面向 types/mod.rs 的 DSL 门面，非执行路径；与 sqlx 版逐字节一致）。
#[derive(Debug, Clone)]
pub enum ParamValue {
    /// SQL NULL。
    Null,
    /// 布尔值。
    Bool(bool),
    /// 整数（i64）。
    Int(i64),
    /// 浮点数（f64）。
    Float(f64),
    /// 字符串。
    String(String),
    /// 高精度十进制数。
    Decimal(Decimal),
    /// UTC 日期时间。
    DateTime(chrono::DateTime<chrono::Utc>),
    /// 无时区日期。
    Date(chrono::NaiveDate),
    /// JSON 值。
    Json(serde_json::Value),
    /// 二进制字节串。
    Binary(Vec<u8>),
    /// UUID。
    Uuid(Uuid),
}

impl ParamValue {
    /// 将 serde_json::Value 转换为 ParamValue（智能类型推断，与 sqlx 版一致）
    pub fn from_json(value: serde_json::Value) -> Self {
        match value {
            serde_json::Value::Null => ParamValue::Null,
            serde_json::Value::Bool(b) => ParamValue::Bool(b),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    ParamValue::Int(i)
                } else if let Some(f) = n.as_f64() {
                    ParamValue::Float(f)
                } else {
                    ParamValue::String(n.to_string())
                }
            }
            serde_json::Value::String(s) => {
                // 1. Uuid
                if let Ok(uuid) = Uuid::parse_str(&s) {
                    return ParamValue::Uuid(uuid);
                }
                // 2. Decimal
                if let Ok(d) = s.parse::<Decimal>() {
                    return ParamValue::Decimal(d);
                }
                // 3. DateTime（多格式）
                if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&s) {
                    return ParamValue::DateTime(dt.with_timezone(&chrono::Utc));
                }
                if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(&s, "%Y-%m-%dT%H:%M:%S%.f") {
                    return ParamValue::DateTime(chrono::DateTime::from_naive_utc_and_offset(
                        dt,
                        chrono::Utc,
                    ));
                }
                if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(&s, "%Y-%m-%dT%H:%M:%S") {
                    return ParamValue::DateTime(chrono::DateTime::from_naive_utc_and_offset(
                        dt,
                        chrono::Utc,
                    ));
                }
                if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S") {
                    return ParamValue::DateTime(chrono::DateTime::from_naive_utc_and_offset(
                        dt,
                        chrono::Utc,
                    ));
                }
                // 4. Date
                if let Ok(date) = chrono::NaiveDate::parse_from_str(&s, "%Y-%m-%d") {
                    return ParamValue::Date(date);
                }
                // 5. base64 Binary
                use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
                if let Ok(bytes) = BASE64.decode(&s) {
                    return ParamValue::Binary(bytes);
                }
                // 6. JSON
                if (s.starts_with('{') || s.starts_with('['))
                    && let Ok(v) = serde_json::from_str::<serde_json::Value>(&s)
                {
                    return ParamValue::Json(v);
                }
                ParamValue::String(s)
            }
            serde_json::Value::Array(arr) => {
                let is_binary = arr.iter().all(|v| {
                    if let serde_json::Value::Number(n) = v {
                        n.as_u64().map(|n| n <= 255).unwrap_or(false)
                    } else {
                        false
                    }
                });
                if is_binary {
                    let bytes: Vec<u8> =
                        arr.iter().filter_map(|v| v.as_u64()).map(|n| n as u8).collect();
                    return ParamValue::Binary(bytes);
                }
                ParamValue::Json(serde_json::Value::Array(arr))
            }
            serde_json::Value::Object(obj) => ParamValue::Json(serde_json::Value::Object(obj)),
        }
    }
}

/// 将 serde_json::Value 数组转换为 Vec<DataValue>（与 sqlx 版一致）
pub fn json_to_data_values(json: serde_json::Value) -> Result<Vec<DataValue>, String> {
    match json {
        serde_json::Value::Array(arr) => arr
            .into_iter()
            .map(|v| serde_json::from_value(v).map_err(|e| e.to_string()))
            .collect(),
        _ => Err("params must be an array".to_string()),
    }
}

// ============================================================================
// 参数绑定：DataValue → Box<dyn ToSql + Sync + Send>
// ============================================================================

/// 将一组 `DataValue` 转换为 tokio-postgres 参数（`Box<dyn ToSql + Sync + Send>`）。
///
/// tokio-postgres 的 `query`/`execute` 需要 `&[&(dyn ToSql + Sync + Send)]`；调用方拿到本
/// `Vec` 后需 `.iter().map(|b| b.as_ref()).collect::<Vec<_>>()` 得引用切片再传入
/// （见 [`as_param_refs`]）。
pub fn bind_data_values_pg(params: &[DataValue]) -> Vec<Box<dyn ToSql + Sync + Send>> {
    params.iter().map(bind_one).collect()
}

/// 把 `Vec<Box<dyn ToSql + Sync + Send>>` 摊平成 tokio-postgres 需要的 `&[&(dyn ToSql + Sync)]`。
///
/// Box 带 `Send` 是为了让持有它的 future 满足 `Send`（api.rs 的 `BoxFuture`）；此处
/// 把引用向下转型为 tokio-postgres 签名要求的 `&(dyn ToSql + Sync)`（去掉 Send 界）。
pub fn as_param_refs(boxed: &[Box<dyn ToSql + Sync + Send>]) -> Vec<&(dyn ToSql + Sync)> {
    boxed
        .iter()
        .map(|b| b.as_ref() as &(dyn ToSql + Sync))
        .collect()
}

fn bind_one(param: &DataValue) -> Box<dyn ToSql + Sync + Send> {
    match param {
        DataValue::Null => Box::new(Option::<String>::None),
        DataValue::NullTyped(t) => match t {
            // 每个 marker 给对的 None::<T>：PG prepare 阶段按 OID 校验 NULL 目标类型，
            // 给错类型会报 "column is of type X but expression is of type text"。
            SqlTypeMarker::Bool => Box::new(Option::<bool>::None),
            SqlTypeMarker::Int => Box::new(PgIntNull), // 宽度自适应 INT2/INT4/INT8 的 NULL
            SqlTypeMarker::Float => Box::new(Option::<f64>::None),
            SqlTypeMarker::Decimal => Box::new(Option::<Decimal>::None),
            SqlTypeMarker::Text => Box::new(Option::<String>::None),
            SqlTypeMarker::Timestamp => Box::new(PgDateTimeNull), // 自适应 TIMESTAMP/TIMESTAMPTZ 的 NULL
            SqlTypeMarker::Date => Box::new(Option::<chrono::NaiveDate>::None),
            SqlTypeMarker::Uuid => Box::new(Option::<Uuid>::None),
            SqlTypeMarker::Json => Box::new(Option::<serde_json::Value>::None),
            SqlTypeMarker::Binary => Box::new(Option::<Vec<u8>>::None),
        },
        DataValue::Bool(v) => Box::new(*v),
        DataValue::Int(v) => Box::new(PgInt(*v)), // 宽度自适应 INT2/INT4/INT8
        DataValue::Float(v) => Box::new(*v), // f64 → FLOAT8
        DataValue::String(v) => Box::new(v.clone()),
        DataValue::Decimal(v) => Box::new(*v),
        DataValue::DateTime(v) => Box::new(PgDateTime(*v)), // 自适应 TIMESTAMP/TIMESTAMPTZ
        DataValue::Date(v) => Box::new(*v),
        DataValue::Json(v) => {
            // Json 承载的是字符串；尝试解析成 Value 绑 JSON/JSONB，失败则当文本。
            match serde_json::from_str::<serde_json::Value>(v) {
                Ok(val) => Box::new(val),
                Err(_) => Box::new(v.clone()),
            }
        }
        DataValue::Binary(v) => Box::new(v.clone()),
        DataValue::Uuid(v) => Box::new(*v),
        DataValue::Array(els) => bind_pg_array(els),
        DataValue::ShortStr(s) => Box::new(s.to_string()),
        DataValue::LongStr(s) => Box::new(s.to_string()),
    }
}

/// 单层同类型数组 → PG 数组（对齐 sqlx 版 `bind_pg_array_postgres`，供 cmx-iam IN 查询）。
///
/// 元素类型由首元素推断；空数组绑 NULL text 数组。
fn bind_pg_array(els: &[DataValue]) -> Box<dyn ToSql + Sync + Send> {
    if els.is_empty() {
        return Box::new(Option::<Vec<String>>::None);
    }
    match &els[0] {
        DataValue::String(_) | DataValue::ShortStr(_) | DataValue::LongStr(_) => {
            let v: Vec<String> = els
                .iter()
                .filter_map(|e| match e {
                    DataValue::String(s) => Some(s.clone()),
                    DataValue::ShortStr(s) | DataValue::LongStr(s) => Some(s.to_string()),
                    _ => None,
                })
                .collect();
            Box::new(v)
        }
        DataValue::Int(_) => {
            let v: Vec<i64> = els
                .iter()
                .filter_map(|e| match e {
                    DataValue::Int(i) => Some(*i),
                    _ => None,
                })
                .collect();
            Box::new(v)
        }
        DataValue::Uuid(_) => {
            let v: Vec<Uuid> = els
                .iter()
                .filter_map(|e| match e {
                    DataValue::Uuid(u) => Some(*u),
                    _ => None,
                })
                .collect();
            Box::new(v)
        }
        _ => Box::new(Option::<Vec<String>>::None),
    }
}

// ============================================================================
// 结果转换：tokio-postgres Row → DataSet
// ============================================================================

/// 结果转换器（tokio-postgres 版）
pub struct PgResultConverter;

impl PgResultConverter {
    /// 将 tokio-postgres 行集合转换为 DataSet
    pub fn convert_rows(rows: Vec<PgRow>, dataset_id: &str) -> DataSet {
        if rows.is_empty() {
            let schema = std::sync::Arc::new(Schema::new_unchecked(dataset_id.to_string(), vec![]));
            return DataSet::empty(dataset_id, schema);
        }

        let first_row = &rows[0];
        let columns = first_row.columns();
        let column_count = columns.len();
        let mut fields = Vec::with_capacity(column_count);

        for column in columns.iter() {
            let column_name = column.name().to_string();
            let field_type = Self::map_type_to_field_type(column.type_(), &column_name);
            fields.push(Field {
                name: column_name,
                field_type,
                label: String::new(),
            });
        }

        let schema = std::sync::Arc::new(Schema::new_unchecked(dataset_id.to_string(), fields));
        let mut dataset = DataSet::with_capacity(dataset_id, schema.clone(), rows.len());

        for row in &rows {
            let mut values = Vec::with_capacity(column_count);
            for i in 0..column_count {
                values.push(Self::get_value_from_row(row, i));
            }
            dataset.add_row(Row::new(values));
        }

        dataset
    }

    /// 从行中按列取值，按 `Type` 精确分派。**任何失败一律 `Null`，绝不 panic。**
    pub(crate) fn get_value_from_row(row: &PgRow, index: usize) -> DataValue {
        let ty = row.columns()[index].type_();

        // 统一“取 Option<T> → 构造 / None / Err 全归 Null”的辅助闭包。
        macro_rules! col {
            ($t:ty, $ctor:expr) => {
                match row.try_get::<usize, Option<$t>>(index) {
                    Ok(Some(v)) => $ctor(v),
                    Ok(None) => DataValue::Null,
                    Err(_) => DataValue::Null,
                }
            };
        }

        match *ty {
            Type::BOOL => col!(bool, DataValue::Bool),
            Type::INT2 => col!(i16, |v| DataValue::Int(v as i64)),
            Type::INT4 => col!(i32, |v| DataValue::Int(v as i64)),
            Type::INT8 => col!(i64, DataValue::Int),
            Type::FLOAT4 => col!(f32, |v| DataValue::Float(v as f64)),
            Type::FLOAT8 => col!(f64, DataValue::Float),
            Type::NUMERIC => col!(Decimal, DataValue::Decimal),
            Type::TEXT | Type::VARCHAR | Type::BPCHAR | Type::NAME | Type::CHAR => {
                col!(String, DataValue::String)
            }
            Type::UUID => col!(Uuid, DataValue::Uuid),
            Type::BYTEA => col!(Vec<u8>, DataValue::Binary),
            Type::JSON | Type::JSONB => {
                col!(serde_json::Value, |v: serde_json::Value| DataValue::Json(v.to_string()))
            }
            Type::DATE => col!(chrono::NaiveDate, DataValue::Date),
            Type::TIMESTAMP => col!(chrono::NaiveDateTime, |ndt| DataValue::DateTime(
                chrono::DateTime::from_naive_utc_and_offset(ndt, chrono::Utc)
            )),
            Type::TIMESTAMPTZ => {
                col!(chrono::DateTime<chrono::Utc>, DataValue::DateTime)
            }
            // 数组类型：还原为 DataValue::Array（元素按标量类型）
            Type::TEXT_ARRAY | Type::VARCHAR_ARRAY | Type::NAME_ARRAY => {
                match row.try_get::<usize, Option<Vec<String>>>(index) {
                    Ok(Some(v)) => {
                        DataValue::Array(v.into_iter().map(DataValue::String).collect())
                    }
                    _ => DataValue::Null,
                }
            }
            Type::INT8_ARRAY | Type::INT4_ARRAY | Type::INT2_ARRAY => {
                match row.try_get::<usize, Option<Vec<i64>>>(index) {
                    Ok(Some(v)) => DataValue::Array(v.into_iter().map(DataValue::Int).collect()),
                    _ => DataValue::Null,
                }
            }
            Type::UUID_ARRAY => match row.try_get::<usize, Option<Vec<Uuid>>>(index) {
                Ok(Some(v)) => DataValue::Array(v.into_iter().map(DataValue::Uuid).collect()),
                _ => DataValue::Null,
            },
            // 其它/未知：兜底当字符串，再失败 Null
            _ => col!(String, DataValue::String),
        }
    }

    /// 将 PG `Type` 映射为 `FieldType`（对齐 sqlx 版 `map_sql_type_to_field_type` 的产出）。
    pub(crate) fn map_type_to_field_type(ty: &Type, column_name: &str) -> FieldType {
        match *ty {
            Type::VARCHAR | Type::TEXT | Type::BPCHAR | Type::CHAR | Type::NAME => {
                FieldType::String
            }
            Type::INT2 | Type::INT4 | Type::INT8 => FieldType::Int,
            Type::FLOAT4 | Type::FLOAT8 => FieldType::Float,
            Type::NUMERIC | Type::MONEY => FieldType::Decimal,
            Type::TIMESTAMP | Type::TIMESTAMPTZ => FieldType::DateTime,
            Type::DATE => FieldType::DateTime,
            Type::BOOL => FieldType::Bool,
            Type::UUID => FieldType::Uuid,
            Type::BYTEA => FieldType::Binary,
            Type::JSON | Type::JSONB => FieldType::Json,
            _ => {
                // 数组类型或未知类型
                let name = ty.name().to_lowercase();
                if name.contains("[]") || name.contains("array") {
                    FieldType::Array
                } else if name.contains("date") || name.contains("time") {
                    FieldType::DateTime
                } else {
                    tracing::warn!(
                        "未处理的数据库字段类型: column={}, type={}",
                        column_name,
                        ty.name()
                    );
                    FieldType::Unknown
                }
            }
        }
    }
}

// ============================================================================
// sea-query Values → Box<dyn ToSql + Sync + Send>（crud 路径）
// ============================================================================

/// 将 `sea_query::Values` 转换为 tokio-postgres 参数。
///
/// 参照 sea-query-sqlx 0.9.1 的 `IntoArguments`（postgres 分支）逐条对齐。
/// 项目未用到的变体返回 `Error::InvalidParams` 而非 panic。
pub fn sea_values_to_tosql(
    values: sea_query::Values,
) -> crate::Result<Vec<Box<dyn ToSql + Sync + Send>>> {
    values.0.into_iter().map(sea_value_to_tosql).collect()
}

fn sea_value_to_tosql(v: sea_query::Value) -> crate::Result<Box<dyn ToSql + Sync + Send>> {
    use sea_query::Value as V;
    let boxed: Box<dyn ToSql + Sync + Send> = match v {
        V::Bool(o) => Box::new(o),
        // PG 无 i8：TinyInt 提升到 i16
        V::TinyInt(o) => Box::new(o.map(|i| i as i16)),
        V::SmallInt(o) => Box::new(o),
        V::Int(o) => Box::new(o),
        V::BigInt(o) => Box::new(o),
        // 无符号 → 有符号提升（对齐 sea-query-sqlx）
        V::TinyUnsigned(o) => Box::new(o.map(|i| i as i16)),
        V::SmallUnsigned(o) => Box::new(o.map(|i| i as i32)),
        V::Unsigned(o) => Box::new(o.map(|i| i as i64)),
        V::BigUnsigned(o) => Box::new(o.map(|i| i as i64)),
        V::Float(o) => Box::new(o),
        V::Double(o) => Box::new(o),
        V::String(o) => Box::new(o), // Option<String>，未装箱
        V::Char(o) => Box::new(o.map(|c| c.to_string())),
        V::Bytes(o) => Box::new(o), // Option<Vec<u8>>，未装箱
        V::Json(o) => Box::new(o.map(|b| *b)), // Option<Box<Json>>
        V::ChronoDate(o) => Box::new(o),
        V::ChronoTime(o) => Box::new(o),
        V::ChronoDateTime(o) => Box::new(o),
        V::ChronoDateTimeUtc(o) => Box::new(o),
        V::ChronoDateTimeLocal(o) => Box::new(o),
        V::ChronoDateTimeWithTimeZone(o) => Box::new(o),
        V::Uuid(o) => Box::new(o), // Option<Uuid>，未装箱
        // 说明：本 crate 的 sea-query 未启用 with-rust_decimal / postgres-array
        // （启用会因 feature 统一破坏旧 crate 依赖的 sea-query-sqlx 的穷尽 match），
        // 故 `Value::Decimal` / `Value::Array` 变体在本编译配置下不存在。crud 层的
        // Decimal 走 DataValue 手写 SQL 路径绑定；若将来 crud 经 sea-query 产出这些
        // 变体，需在全 workspace 协调启用对应 feature 后在此补分支。
        other => {
            return Err(crate::Error::InvalidParams(format!(
                "sea_query::Value 变体暂不支持转换为 tokio-postgres 参数: {:?}",
                other
            )));
        }
    };
    Ok(boxed)
}
