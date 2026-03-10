/// 结果转换模块，用于处理SQLx查询结果到DataSet的转换
/// 
/// 该模块提供了独立的结果转换功能，不依赖于 DbTransaction

use cmx_core::model::data::dataset::{DataSet, Row, Schema};
use cmx_core::model::cell::{DataValue, Field, FieldType};
use sqlx::{Row as SqlxRow, Column};
use rust_decimal::Decimal;
use std::str::FromStr;

/// 参数值类型，用于 SQL 参数绑定
#[derive(Debug, Clone)]
pub enum ParamValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Decimal(Decimal),
    DateTime(chrono::NaiveDateTime),
    Date(chrono::NaiveDate),
    Json(serde_json::Value),
}

impl ParamValue {
    /// 将 serde_json::Value 转换为 ParamValue
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
            serde_json::Value::String(s) => ParamValue::String(s),
            serde_json::Value::Array(arr) => ParamValue::Json(serde_json::Value::Array(arr)),
            serde_json::Value::Object(obj) => ParamValue::Json(serde_json::Value::Object(obj)),
        }
    }
}

/// 结果转换器
pub struct ResultConverter;

impl ResultConverter {
    /// 将 PostgreSQL 行转换为 DataSet
    pub fn convert_postgres_rows(rows: Vec<sqlx::postgres::PgRow>, dataset_id: &str) -> DataSet {
        if rows.is_empty() {
            let schema = std::sync::Arc::new(Schema::new(dataset_id.to_string(), vec![]));
            return DataSet::empty(dataset_id, schema);
        }

        let first_row = &rows[0];
        let column_count = first_row.columns().len();
        let mut fields = Vec::with_capacity(column_count);

        for column in first_row.columns().iter() {
            let column_name = column.name().to_string();
            let field_type = Self::map_sql_type_to_field_type(column.type_info());
            fields.push(Field {
                name: column_name,
                field_type,
                label: String::new(),
            });
        }

        let schema = std::sync::Arc::new(Schema::new(dataset_id.to_string(), fields));
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
            let schema = std::sync::Arc::new(Schema::new(dataset_id.to_string(), vec![]));
            return DataSet::empty(dataset_id, schema);
        }

        let first_row = &rows[0];
        let column_count = first_row.columns().len();
        let mut fields = Vec::with_capacity(column_count);

        for column in first_row.columns().iter() {
            let column_name = column.name().to_string();
            let field_type = Self::map_sql_type_to_field_type(column.type_info());
            fields.push(Field {
                name: column_name,
                field_type,
                label: String::new(),
            });
        }

        let schema = std::sync::Arc::new(Schema::new(dataset_id.to_string(), fields));
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
            let schema = std::sync::Arc::new(Schema::new(dataset_id.to_string(), vec![]));
            return DataSet::empty(dataset_id, schema);
        }

        let first_row = &rows[0];
        let column_count = first_row.columns().len();
        let mut fields = Vec::with_capacity(column_count);

        for column in first_row.columns().iter() {
            let column_name = column.name().to_string();
            let field_type = Self::map_sql_type_to_field_type(column.type_info());
            fields.push(Field {
                name: column_name,
                field_type,
                label: String::new(),
            });
        }

        let schema = std::sync::Arc::new(Schema::new(dataset_id.to_string(), fields));
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
    fn get_postgres_value_from_row(row: &sqlx::postgres::PgRow, index: usize) -> DataValue {
        if let Ok(value) = row.try_get::<String, _>(index) {
            DataValue::String(value)
        } else if let Ok(value) = row.try_get::<i64, _>(index) {
            DataValue::Int(value)
        } else if let Ok(value) = row.try_get::<f64, _>(index) {
            DataValue::Float(value)
        } else if let Ok(value) = row.try_get::<bool, _>(index) {
            DataValue::Bool(value)
        } else {
            match row.try_get::<String, _>(index) {
                Ok(value) => {
                    if let Ok(decimal) = Decimal::from_str(&value) {
                        DataValue::Decimal(decimal)
                    } else {
                        DataValue::String(value)
                    }
                },
                Err(_) => DataValue::Null,
            }
        }
    }

    /// 从 MySQL 行中获取值
    fn get_mysql_value_from_row(row: &sqlx::mysql::MySqlRow, index: usize) -> DataValue {
        if let Ok(value) = row.try_get::<String, _>(index) {
            DataValue::String(value)
        } else if let Ok(value) = row.try_get::<i64, _>(index) {
            DataValue::Int(value)
        } else if let Ok(value) = row.try_get::<f64, _>(index) {
            DataValue::Float(value)
        } else if let Ok(value) = row.try_get::<bool, _>(index) {
            DataValue::Bool(value)
        } else {
            match row.try_get::<String, _>(index) {
                Ok(value) => {
                    if let Ok(decimal) = Decimal::from_str(&value) {
                        DataValue::Decimal(decimal)
                    } else {
                        DataValue::String(value)
                    }
                },
                Err(_) => DataValue::Null,
            }
        }
    }

    /// 从 SQLite 行中获取值
    fn get_sqlite_value_from_row(row: &sqlx::sqlite::SqliteRow, index: usize) -> DataValue {
        if let Ok(value) = row.try_get::<String, _>(index) {
            DataValue::String(value)
        } else if let Ok(value) = row.try_get::<i64, _>(index) {
            DataValue::Int(value)
        } else if let Ok(value) = row.try_get::<f64, _>(index) {
            DataValue::Float(value)
        } else if let Ok(value) = row.try_get::<bool, _>(index) {
            DataValue::Bool(value)
        } else {
            match row.try_get::<String, _>(index) {
                Ok(value) => {
                    if let Ok(decimal) = Decimal::from_str(&value) {
                        DataValue::Decimal(decimal)
                    } else {
                        DataValue::String(value)
                    }
                },
                Err(_) => DataValue::Null,
            }
        }
    }

    /// 将 SQL 类型映射为 FieldType
    fn map_sql_type_to_field_type(type_info: &impl sqlx::TypeInfo) -> FieldType {
        let type_name = format!("{}", type_info);
        let type_name_lower = type_name.to_lowercase();
        
        if type_name_lower.contains("varchar") || type_name_lower.contains("text") 
            || type_name_lower.contains("string") || type_name_lower.contains("char") {
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
        } else {
            FieldType::String
        }
    }
}
