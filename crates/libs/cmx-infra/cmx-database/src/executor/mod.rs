use cmx_core::model::data::dataset::{DataSet, Row, Schema};
use cmx_core::model::cell::{DataValue, Field, FieldType};
use sqlx::{Row as SqlxRow, Column};
use rust_decimal::Decimal;
use uuid::Uuid;

/// 参数值类型，用于 SQL 参数绑定
#[derive(Debug, Clone)]
pub enum ParamValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Decimal(Decimal),
    DateTime(chrono::DateTime<chrono::Utc>),
    Date(chrono::NaiveDate),
    Json(serde_json::Value),
    Binary(Vec<u8>),
    Uuid(Uuid),
}

impl ParamValue {
    /// 将 serde_json::Value 转换为 ParamValue
    ///
    /// 支持的类型转换：
    /// - null -> Null
    /// - bool -> Bool
    /// - i64 -> Int
    /// - f64 -> Float
    /// - 数字字符串 -> Decimal（如果可以解析）
    /// - 日期时间字符串 -> DateTime（支持多种格式）
    /// - 日期字符串 -> Date
    /// - UUID 格式字符串 -> Uuid
    /// - base64 编码字符串 -> Binary
    /// - 数组/对象 -> Json
    /// - 普通字符串 -> String
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
                // 尝试解析为特定类型

                // 1. 尝试解析为 Uuid
                if let Ok(uuid) = Uuid::parse_str(&s) {
                    return ParamValue::Uuid(uuid);
                }

                // 2. 尝试解析为 Decimal（金额等）
                if let Ok(d) = s.parse::<Decimal>() {
                    return ParamValue::Decimal(d);
                }

                // 3. 尝试解析为 DateTime（支持多种格式）
                if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&s) {
                    return ParamValue::DateTime(dt.with_timezone(&chrono::Utc));
                }
                if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(&s, "%Y-%m-%dT%H:%M:%S%.f") {
                    return ParamValue::DateTime(chrono::DateTime::from_naive_utc_and_offset(dt, chrono::Utc));
                }
                if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(&s, "%Y-%m-%dT%H:%M:%S") {
                    return ParamValue::DateTime(chrono::DateTime::from_naive_utc_and_offset(dt, chrono::Utc));
                }
                if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S") {
                    return ParamValue::DateTime(chrono::DateTime::from_naive_utc_and_offset(dt, chrono::Utc));
                }

                // 4. 尝试解析为 Date
                if let Ok(date) = chrono::NaiveDate::parse_from_str(&s, "%Y-%m-%d") {
                    return ParamValue::Date(date);
                }

                // 5. 尝试解析为 base64 Binary
                use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
                if let Ok(bytes) = BASE64.decode(&s) {
                    return ParamValue::Binary(bytes);
                }

                // 6. 尝试解析为 JSON（以 { 或 [ 开头）
                if (s.starts_with('{') || s.starts_with('['))
                    && let Ok(v) = serde_json::from_str::<serde_json::Value>(&s) {
                        return ParamValue::Json(v);
                    }

                // 默认作为字符串
                ParamValue::String(s)
            }
            serde_json::Value::Array(arr) => {
                // 检查是否为二进制数据（全是 0-255 的数字）
                let is_binary = arr.iter().all(|v| {
                    if let serde_json::Value::Number(n) = v {
                        n.as_u64().map(|n| n <= 255).unwrap_or(false)
                    } else {
                        false
                    }
                });

                if is_binary {
                    let bytes: Vec<u8> = arr.iter()
                        .filter_map(|v| v.as_u64())
                        .map(|n| n as u8)
                        .collect();
                    return ParamValue::Binary(bytes);
                }

                // 否则作为 JSON 数组
                ParamValue::Json(serde_json::Value::Array(arr))
            }
            serde_json::Value::Object(obj) => {
                // 对象转为 JSON
                ParamValue::Json(serde_json::Value::Object(obj))
            }
        }
    }
}

/// DataValue 绑定函数：将 DataValue 绑定到 sqlx 查询（PostgreSQL）
///
/// 识别 `DataValue::NullTyped` 携带的类型信息,绑定正确的 sqlx 类型,
/// 避免 `None::<String>` 导致非 TEXT 列的 prepare 类型不匹配。
#[inline]
pub fn bind_data_value_postgres<'q>(
    query: sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
    param: &'q DataValue,
) -> sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments> {
    use cmx_core::model::cell::SqlTypeMarker;
    match param {
        DataValue::Null => query.bind(None::<String>),
        DataValue::NullTyped(t) => match t {
            SqlTypeMarker::Bool      => query.bind(None::<bool>),
            SqlTypeMarker::Int       => query.bind(None::<i64>),
            SqlTypeMarker::Float     => query.bind(None::<f64>),
            SqlTypeMarker::Decimal   => query.bind(None::<rust_decimal::Decimal>),
            SqlTypeMarker::Text      => query.bind(None::<String>),
            SqlTypeMarker::Timestamp => query.bind(None::<chrono::DateTime<chrono::Utc>>),
            SqlTypeMarker::Date      => query.bind(None::<chrono::NaiveDate>),
            SqlTypeMarker::Uuid      => query.bind(None::<uuid::Uuid>),
            SqlTypeMarker::Json      => query.bind(None::<serde_json::Value>),
            SqlTypeMarker::Binary    => query.bind(None::<Vec<u8>>),
        },
        DataValue::Bool(v)    => query.bind(*v),
        DataValue::Int(v)     => query.bind(*v),
        DataValue::Float(v)   => query.bind(*v),
        DataValue::String(v)  => query.bind(v.as_str()),
        DataValue::Decimal(v) => query.bind(*v),
        DataValue::DateTime(v) => query.bind(*v),
        DataValue::Date(v)    => query.bind(*v),
        DataValue::Json(v)    => query.bind(v.clone()),
        DataValue::Binary(v)  => query.bind(v.as_slice()),
        DataValue::Uuid(v)    => query.bind(*v),
        DataValue::Array(els) => bind_pg_array_postgres(query, els),
        DataValue::ShortStr(s) => query.bind(s.as_str()),
        DataValue::LongStr(s)  => query.bind(s.as_str()),
    }
}

/// 将单层同类型数组绑定为 PostgreSQL 数组。
///
/// 元素类型由首个元素推断;空数组绑定为 NULL text 数组。
/// 仅支持单层、元素同类型(对应 cmx-iam 的 IN 查询场景)。
fn bind_pg_array_postgres<'q>(
    query: sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
    els: &'q [DataValue],
) -> sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments> {
    if els.is_empty() {
        return query.bind(None::<Vec<String>>);
    }
    // 按首个元素类型分发
    match &els[0] {
        DataValue::String(_) | DataValue::ShortStr(_) | DataValue::LongStr(_) => {
            let v: Vec<&str> = els.iter().filter_map(|e| match e {
                DataValue::String(s) => Some(s.as_str()),
                DataValue::ShortStr(s) => Some(s.as_str()),
                DataValue::LongStr(s) => Some(s.as_str()),
                _ => None,
            }).collect();
            query.bind(v)
        }
        DataValue::Int(_) => {
            let v: Vec<i64> = els.iter().filter_map(|e| match e {
                DataValue::Int(i) => Some(*i),
                _ => None,
            }).collect();
            query.bind(v)
        }
        DataValue::Uuid(_) => {
            let v: Vec<uuid::Uuid> = els.iter().filter_map(|e| match e {
                DataValue::Uuid(u) => Some(*u),
                _ => None,
            }).collect();
            query.bind(v)
        }
        _ => query.bind(None::<Vec<String>>),
    }
}

/// DataValue 绑定函数：将 DataValue 绑定到 sqlx 查询（MySQL）
///
/// MySQL 的 NULL 无类型区分,`NullTyped` 统一走 `None::<String>` 兜底。
/// MySQL 无原生数组,`Array` 序列化为逗号分隔字符串。
#[inline]
pub fn bind_data_value_mysql<'q>(
    query: sqlx::query::Query<'q, sqlx::MySql, sqlx::mysql::MySqlArguments>,
    param: &'q DataValue,
) -> sqlx::query::Query<'q, sqlx::MySql, sqlx::mysql::MySqlArguments> {
    match param {
        DataValue::Null | DataValue::NullTyped(_) => query.bind(None::<String>),
        DataValue::Bool(v)    => query.bind(*v),
        DataValue::Int(v)     => query.bind(*v),
        DataValue::Float(v)   => query.bind(*v),
        DataValue::String(v)  => query.bind(v.clone()),
        DataValue::Decimal(v) => query.bind(v.to_string()),
        DataValue::DateTime(v) => query.bind(v.to_rfc3339()),
        DataValue::Date(v)    => query.bind(v.to_string()),
        DataValue::Json(v)    => query.bind(v.to_string()),
        DataValue::Binary(v)  => query.bind(v.as_slice()),
        DataValue::Uuid(v)    => query.bind(v.to_string()),
        DataValue::Array(els) => {
            let s = els.iter().map(|e| match e {
                DataValue::String(s) => s.clone(),
                DataValue::ShortStr(s) | DataValue::LongStr(s) => s.to_string(),
                other => format!("{:?}", other),
            }).collect::<Vec<_>>().join(",");
            query.bind(s)
        }
        DataValue::ShortStr(s) => query.bind(s.to_string()),
        DataValue::LongStr(s)  => query.bind(s.to_string()),
    }
}

/// DataValue 绑定函数：将 DataValue 绑定到 sqlx 查询（SQLite）
///
/// SQLite 动态类型,NULL 无类型区分,`NullTyped` 统一走 `None::<String>` 兜底。
/// `Array` 序列化为 JSON 字符串。
#[inline]
pub fn bind_data_value_sqlite<'q>(
    query: sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments>,
    param: &'q DataValue,
) -> sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments> {
    match param {
        DataValue::Null | DataValue::NullTyped(_) => query.bind(None::<String>),
        DataValue::Bool(v)    => query.bind(*v),
        DataValue::Int(v)     => query.bind(*v),
        DataValue::Float(v)   => query.bind(*v),
        DataValue::String(v)  => query.bind(v.clone()),
        DataValue::Decimal(v) => query.bind(v.to_string()),
        DataValue::DateTime(v) => query.bind(v.to_rfc3339()),
        DataValue::Date(v)    => query.bind(v.to_string()),
        DataValue::Json(v)    => query.bind(v.to_string()),
        DataValue::Binary(v)  => query.bind(v.as_slice()),
        DataValue::Uuid(v)    => query.bind(v.to_string()),
        DataValue::Array(els) => query.bind(serde_json::to_string(els).unwrap_or_default()),
        DataValue::ShortStr(s) => query.bind(s.to_string()),
        DataValue::LongStr(s)  => query.bind(s.to_string()),
    }
}

/// 将 serde_json::Value 数组转换为 Vec<DataValue>
pub fn json_to_data_values(json: serde_json::Value) -> Result<Vec<DataValue>, String> {
    match json {
        serde_json::Value::Array(arr) => {
            arr.into_iter()
                .map(|v| serde_json::from_value(v).map_err(|e| e.to_string()))
                .collect()
        }
        _ => Err("params must be an array".to_string()),
    }
}

/// 结果转换器
pub struct ResultConverter;

impl ResultConverter {
    /// 将 PostgreSQL 行转换为 DataSet
    pub fn convert_postgres_rows(rows: Vec<sqlx::postgres::PgRow>, dataset_id: &str) -> DataSet {
        if rows.is_empty() {
            let schema = std::sync::Arc::new(Schema::new_unchecked(dataset_id.to_string(), vec![]));
            return DataSet::empty(dataset_id, schema);
        }

        let first_row = &rows[0];
        let column_count = first_row.columns().len();
        let mut fields = Vec::with_capacity(column_count);

        for column in first_row.columns().iter() {
            let column_name = column.name().to_string();
            let field_type = Self::map_sql_type_to_field_type(column.type_info(), &column_name);
            fields.push(Field {
                name: column_name,
                field_type,
                label: String::new(),
            });
        }

        let schema = std::sync::Arc::new(Schema::new_unchecked(dataset_id.to_string(), fields));
        let mut dataset = DataSet::with_capacity(dataset_id, schema.clone(), rows.len());

        for row in rows {
            let mut values = Vec::with_capacity(column_count);
            for i in 0..column_count {
                let value = Self::get_postgres_value_from_row(&row, i);
                values.push(value);
            }
            dataset.add_row(Row::new(values));
        }

        dataset
    }

    /// 将 MySQL 行转换为 DataSet
    pub fn convert_mysql_rows(rows: Vec<sqlx::mysql::MySqlRow>, dataset_id: &str) -> DataSet {
        if rows.is_empty() {
            let schema = std::sync::Arc::new(Schema::new_unchecked(dataset_id.to_string(), vec![]));
            return DataSet::empty(dataset_id, schema);
        }

        let first_row = &rows[0];
        let column_count = first_row.columns().len();
        let mut fields = Vec::with_capacity(column_count);

        for column in first_row.columns().iter() {
            let column_name = column.name().to_string();
            let field_type = Self::map_sql_type_to_field_type(column.type_info(), &column_name);
            fields.push(Field {
                name: column_name,
                field_type,
                label: String::new(),
            });
        }

        let schema = std::sync::Arc::new(Schema::new_unchecked(dataset_id.to_string(), fields));
        let mut dataset = DataSet::with_capacity(dataset_id, schema.clone(), rows.len());

        for row in rows {
            let mut values = Vec::with_capacity(column_count);
            for i in 0..column_count {
                let value = Self::get_mysql_value_from_row(&row, i);
                values.push(value);
            }
            dataset.add_row(Row::new(values));
        }

        dataset
    }

    /// 将 SQLite 行转换为 DataSet
    pub fn convert_sqlite_rows(rows: Vec<sqlx::sqlite::SqliteRow>, dataset_id: &str) -> DataSet {
        if rows.is_empty() {
            let schema = std::sync::Arc::new(Schema::new_unchecked(dataset_id.to_string(), vec![]));
            return DataSet::empty(dataset_id, schema);
        }

        let first_row = &rows[0];
        let column_count = first_row.columns().len();
        let mut fields = Vec::with_capacity(column_count);

        for column in first_row.columns().iter() {
            let column_name = column.name().to_string();
            let field_type = Self::map_sql_type_to_field_type(column.type_info(), &column_name);
            fields.push(Field {
                name: column_name,
                field_type,
                label: String::new(),
            });
        }

        let schema = std::sync::Arc::new(Schema::new_unchecked(dataset_id.to_string(), fields));
        let mut dataset = DataSet::with_capacity(dataset_id, schema.clone(), rows.len());

        for row in rows {
            let mut values = Vec::with_capacity(column_count);
            for i in 0..column_count {
                let value = Self::get_sqlite_value_from_row(&row, i);
                values.push(value);
            }
            dataset.add_row(Row::new(values));
        }

        dataset
    }

    /// 从 PostgreSQL 行中获取值
    pub(crate) fn get_postgres_value_from_row(row: &sqlx::postgres::PgRow, index: usize) -> DataValue {
        let type_info = row.column(index).type_info();
        let type_name = type_info.to_string().to_lowercase();

        if type_name.contains("int") {
            // 尝试 i32，如果失败尝试 i64
            if let Ok(v) = row.try_get::<i32, _>(index) {
                return DataValue::Int(v as i64);
            }
            if let Ok(v) = row.try_get::<i64, _>(index) {
                return DataValue::Int(v);
            }
            if let Ok(v) = row.try_get::<i16, _>(index) {
                return DataValue::Int(v as i64);
            }
            DataValue::Null
        } else if type_name.contains("float") || type_name.contains("double") || type_name.contains("real") {
            row.try_get::<f64, _>(index).map(DataValue::Float).unwrap_or(DataValue::Null)
        } else if type_name.contains("bool") {
            row.try_get::<bool, _>(index).map(DataValue::Bool).unwrap_or(DataValue::Null)
        } else if type_name.contains("decimal") || type_name.contains("numeric") {
            row.try_get::<String, _>(index)
                .ok()
                .and_then(|s| s.parse::<rust_decimal::Decimal>().ok())
                .map(DataValue::Decimal)
                .unwrap_or(DataValue::Null)
        } else if type_name.contains("uuid") {
            // 处理 UUID 类型
            row.try_get::<Uuid, _>(index)
                .map(DataValue::Uuid)
                .unwrap_or(DataValue::Null)
        } else if type_name.contains("bytea") || type_name.contains("blob") {
            // 处理二进制类型
            row.try_get::<Vec<u8>, _>(index)
                .map(DataValue::Binary)
                .unwrap_or(DataValue::Null)
        } else if type_name.contains("json") || type_name.contains("jsonb") {
            // 处理 JSON/JSONB 类型 - 优先尝试直接获取 serde_json::Value
            if let Ok(json_val) = row.try_get::<serde_json::Value, _>(index) {
                return DataValue::Json(json_val.to_string());
            }
            // 如果失败，尝试作为 String 获取
            row.try_get::<String, _>(index)
                .map(DataValue::Json)
                .unwrap_or(DataValue::Null)
        } else if type_name.contains("date") && !type_name.contains("time") {
            // 处理纯日期类型 - 作为字符串获取后解析
            if let Ok(s) = row.try_get::<String, _>(index)
                && let Ok(date) = chrono::NaiveDate::parse_from_str(&s, "%Y-%m-%d") {
                    return DataValue::Date(date);
                }
            DataValue::Null
        } else if type_name.contains("timestamp") || type_name.contains("datetime") {
            // 处理时间戳类型 - 直接使用 chrono 类型获取（sqlx chrono feature 启用）
            // 优先尝试直接获取带时区的 DateTime<Utc>
            if let Ok(dt) = row.try_get::<chrono::DateTime<chrono::Utc>, _>(index) {
                return DataValue::DateTime(dt);
            }
            // 尝试获取 NaiveDateTime 然后转换为 DateTime<Utc>
            if let Ok(ndt) = row.try_get::<chrono::NaiveDateTime, _>(index) {
                return DataValue::DateTime(chrono::DateTime::from_naive_utc_and_offset(ndt, chrono::Utc));
            }
            // 如果都失败，作为字符串处理
            if let Ok(s) = row.try_get::<String, _>(index) {
                if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&s) {
                    return DataValue::DateTime(dt.with_timezone(&chrono::Utc));
                }
                if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S%.f") {
                    return DataValue::DateTime(chrono::DateTime::from_naive_utc_and_offset(dt, chrono::Utc));
                }
                if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S") {
                    return DataValue::DateTime(chrono::DateTime::from_naive_utc_and_offset(dt, chrono::Utc));
                }
                return DataValue::String(s);
            }
            DataValue::Null
        } else {
            // 默认尝试获取字符串
            row.try_get::<String, _>(index).map(DataValue::String).unwrap_or(DataValue::Null)
        }
    }

    /// 从 MySQL 行中获取值
    pub(crate) fn get_mysql_value_from_row(row: &sqlx::mysql::MySqlRow, index: usize) -> DataValue {
        let type_info = row.column(index).type_info();
        let type_name = type_info.to_string().to_lowercase();

        if type_name.contains("int") {
            // 尝试 i32，如果失败尝试 i64
            if let Ok(v) = row.try_get::<i32, _>(index) {
                return DataValue::Int(v as i64);
            }
            if let Ok(v) = row.try_get::<i64, _>(index) {
                return DataValue::Int(v);
            }
            if let Ok(v) = row.try_get::<i16, _>(index) {
                return DataValue::Int(v as i64);
            }
            DataValue::Null
        } else if type_name.contains("float") || type_name.contains("double") || type_name.contains("real") {
            row.try_get::<f64, _>(index).map(DataValue::Float).unwrap_or(DataValue::Null)
        } else if type_name.contains("bool") {
            row.try_get::<bool, _>(index).map(DataValue::Bool).unwrap_or(DataValue::Null)
        } else if type_name.contains("decimal") || type_name.contains("numeric") {
            row.try_get::<String, _>(index)
                .ok()
                .and_then(|s| s.parse::<rust_decimal::Decimal>().ok())
                .map(DataValue::Decimal)
                .unwrap_or(DataValue::Null)
        } else if type_name.contains("char") && type_name.contains("uuid") {
            // 处理 MySQL 的 UUID（可能以 CHAR 形式存储）
            row.try_get::<String, _>(index)
                .ok()
                .and_then(|s| Uuid::parse_str(&s).ok())
                .map(DataValue::Uuid)
                .unwrap_or(DataValue::Null)
        } else if type_name.contains("binary") || type_name.contains("blob") || type_name.contains("bytea") {
            // 处理二进制类型
            row.try_get::<Vec<u8>, _>(index)
                .map(DataValue::Binary)
                .unwrap_or(DataValue::Null)
        } else if type_name.contains("json") {
            // 处理 JSON 类型
            row.try_get::<String, _>(index)
                .map(DataValue::Json)
                .unwrap_or(DataValue::Null)
        } else if type_name.contains("date") && !type_name.contains("time") {
            // 处理纯日期类型 - 作为字符串获取后解析
            if let Ok(s) = row.try_get::<String, _>(index)
                && let Ok(date) = chrono::NaiveDate::parse_from_str(&s, "%Y-%m-%d") {
                    return DataValue::Date(date);
                }
            DataValue::Null
        } else if type_name.contains("timestamp") || type_name.contains("datetime") {
            // 处理时间戳类型 - 直接使用 chrono 类型获取（sqlx chrono feature 启用）
            // 优先尝试直接获取带时区的 DateTime<Utc>
            if let Ok(dt) = row.try_get::<chrono::DateTime<chrono::Utc>, _>(index) {
                return DataValue::DateTime(dt);
            }
            // 尝试获取 NaiveDateTime 然后转换为 DateTime<Utc>
            if let Ok(ndt) = row.try_get::<chrono::NaiveDateTime, _>(index) {
                return DataValue::DateTime(chrono::DateTime::from_naive_utc_and_offset(ndt, chrono::Utc));
            }
            // 如果都失败，作为字符串处理
            if let Ok(s) = row.try_get::<String, _>(index) {
                if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&s) {
                    return DataValue::DateTime(dt.with_timezone(&chrono::Utc));
                }
                if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S%.f") {
                    return DataValue::DateTime(chrono::DateTime::from_naive_utc_and_offset(dt, chrono::Utc));
                }
                if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S") {
                    return DataValue::DateTime(chrono::DateTime::from_naive_utc_and_offset(dt, chrono::Utc));
                }
                return DataValue::String(s);
            }
            DataValue::Null
        } else {
            // 默认尝试获取字符串
            row.try_get::<String, _>(index).map(DataValue::String).unwrap_or(DataValue::Null)
        }
    }

    /// 从 SQLite 行中获取值
    pub(crate) fn get_sqlite_value_from_row(row: &sqlx::sqlite::SqliteRow, index: usize) -> DataValue {
        let type_info = row.column(index).type_info();
        let type_name = type_info.to_string().to_lowercase();

        if type_name.contains("int") {
            // 尝试 i32，如果失败尝试 i64
            if let Ok(v) = row.try_get::<i32, _>(index) {
                return DataValue::Int(v as i64);
            }
            if let Ok(v) = row.try_get::<i64, _>(index) {
                return DataValue::Int(v);
            }
            if let Ok(v) = row.try_get::<i16, _>(index) {
                return DataValue::Int(v as i64);
            }
            DataValue::Null
        } else if type_name.contains("float") || type_name.contains("double") || type_name.contains("real") {
            row.try_get::<f64, _>(index).map(DataValue::Float).unwrap_or(DataValue::Null)
        } else if type_name.contains("bool") {
            row.try_get::<bool, _>(index).map(DataValue::Bool).unwrap_or(DataValue::Null)
        } else if type_name.contains("decimal") || type_name.contains("numeric") {
            row.try_get::<String, _>(index)
                .ok()
                .and_then(|s| s.parse::<rust_decimal::Decimal>().ok())
                .map(DataValue::Decimal)
                .unwrap_or(DataValue::Null)
        } else if type_name.contains("blob") || type_name.contains("binary") {
            // 处理 SQLite 的二进制类型
            row.try_get::<Vec<u8>, _>(index)
                .map(DataValue::Binary)
                .unwrap_or(DataValue::Null)
        } else if type_name.contains("json") {
            // 处理 SQLite 的 JSON 类型
            row.try_get::<String, _>(index)
                .map(DataValue::Json)
                .unwrap_or(DataValue::Null)
        } else if type_name.contains("date") && !type_name.contains("time") {
            // 处理纯日期类型 - 作为字符串获取后解析
            if let Ok(s) = row.try_get::<String, _>(index)
                && let Ok(date) = chrono::NaiveDate::parse_from_str(&s, "%Y-%m-%d") {
                    return DataValue::Date(date);
                }
            DataValue::Null
        } else if type_name.contains("timestamp") || type_name.contains("datetime") {
            // 处理时间戳类型 - 直接使用 chrono 类型获取（sqlx chrono feature 启用）
            // 优先尝试直接获取 NaiveDateTime
            if let Ok(ndt) = row.try_get::<chrono::NaiveDateTime, _>(index) {
                return DataValue::DateTime(chrono::DateTime::from_naive_utc_and_offset(ndt, chrono::Utc));
            }
            // 如果都失败，作为字符串处理
            if let Ok(s) = row.try_get::<String, _>(index) {
                if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&s) {
                    return DataValue::DateTime(dt.with_timezone(&chrono::Utc));
                }
                if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S%.f") {
                    return DataValue::DateTime(chrono::DateTime::from_naive_utc_and_offset(dt, chrono::Utc));
                }
                if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S") {
                    return DataValue::DateTime(chrono::DateTime::from_naive_utc_and_offset(dt, chrono::Utc));
                }
                return DataValue::String(s);
            }
            DataValue::Null
        } else {
            // 默认尝试获取字符串
            row.try_get::<String, _>(index).map(DataValue::String).unwrap_or(DataValue::Null)
        }
    }

    /// 将 SQL 类型映射为 FieldType
    pub(crate) fn map_sql_type_to_field_type(type_info: &impl sqlx::TypeInfo, column_name: &str) -> FieldType {
        let type_name = format!("{}", type_info);
        let type_name_lower = type_name.to_lowercase();

        //NAME 是 PostgreSQL 的标识符类型（字符串类似），把 name 加到字符串匹配分支即可， NAME[] 也会被 contains("name") 覆盖。
        if type_name_lower.contains("varchar") || type_name_lower.contains("text")
            || type_name_lower.contains("string") || type_name_lower.contains("char")
            || type_name_lower.contains("name") {
            FieldType::String
        } else if type_name_lower.contains("int") || type_name_lower.contains("bigint")
            || type_name_lower.contains("smallint") || type_name_lower.contains("tinyint") {
            FieldType::Int
        } else if type_name_lower.contains("float") || type_name_lower.contains("double")
            || type_name_lower.contains("real") {
            FieldType::Float
        } else if type_name_lower.contains("decimal") || type_name_lower.contains("numeric")
            || type_name_lower.contains("money") {
            FieldType::Decimal
        } else if type_name_lower.contains("timestamp") || type_name_lower.contains("datetime") {
            FieldType::DateTime
        } else if type_name_lower.contains("bool") {
            FieldType::Bool
        } else if type_name_lower.contains("uuid") {
            FieldType::Uuid
        } else if type_name_lower.contains("bytea") || type_name_lower.contains("blob") || type_name_lower.contains("binary") {
            FieldType::Binary
        } else if type_name_lower.contains("json") {
            FieldType::Json
        } else if type_name_lower.contains("array") {
            FieldType::Array
        } else {
            tracing::warn!("未处理的数据库字段类型: column={}, type={}", column_name, type_name);
            FieldType::Unknown
        }
    }
}
