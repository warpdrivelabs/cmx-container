/// 事务转换模块，用于处理SQLx查询结果到DataSet的转换
///
/// 该模块定义了类型转换的trait和实现，包括：
/// - TransactionConverter trait：定义转换方法
/// - DbTransaction的转换实现
/// - 不同数据库类型的转换方法

use cmx_core::model::data::dataset::{DataSet, Row, Schema};
use cmx_core::model::cell::{DataValue, Field, FieldType};
use sqlx::{Row as SqlxRow, Column};
use rust_decimal::Decimal;
use std::str::FromStr;
use crate::transaction::core::DbTransaction;

/// 事务转换器trait，定义转换方法
pub trait TransactionConverter {
    /// 将PostgreSQL行转换为DataSet
    ///
    /// # 参数
    /// * `rows` - PostgreSQL查询结果行
    /// * `dataset_id` - 数据集唯一标识
    ///
    /// # 返回值
    /// * `DataSet` - 转换后的数据集
    fn convert_postgres_rows_to_dataset(&self, rows: Vec<sqlx::postgres::PgRow>, dataset_id: &str) -> DataSet;

    /// 将MySQL行转换为DataSet
    ///
    /// # 参数
    /// * `rows` - MySQL查询结果行
    /// * `dataset_id` - 数据集唯一标识
    ///
    /// # 返回值
    /// * `DataSet` - 转换后的数据集
    fn convert_mysql_rows_to_dataset(&self, rows: Vec<sqlx::mysql::MySqlRow>, dataset_id: &str) -> DataSet;

    /// 将SQLite行转换为DataSet
    ///
    /// # 参数
    /// * `rows` - SQLite查询结果行
    /// * `dataset_id` - 数据集唯一标识
    ///
    /// # 返回值
    /// * `DataSet` - 转换后的数据集
    fn convert_sqlite_rows_to_dataset(&self, rows: Vec<sqlx::sqlite::SqliteRow>, dataset_id: &str) -> DataSet;

    /// 从PostgreSQL行中获取值
    ///
    /// # 参数
    /// * `row` - PostgreSQL行
    /// * `index` - 列索引
    ///
    /// # 返回值
    /// * `DataValue` - 转换后的数据值
    fn get_postgres_value_from_row(&self, row: &sqlx::postgres::PgRow, index: usize) -> DataValue;

    /// 从MySQL行中获取值
    ///
    /// # 参数
    /// * `row` - MySQL行
    /// * `index` - 列索引
    ///
    /// # 返回值
    /// * `DataValue` - 转换后的数据值
    fn get_mysql_value_from_row(&self, row: &sqlx::mysql::MySqlRow, index: usize) -> DataValue;

    /// 从SQLite行中获取值
    ///
    /// # 参数
    /// * `row` - SQLite行
    /// * `index` - 列索引
    ///
    /// # 返回值
    /// * `DataValue` - 转换后的数据值
    fn get_sqlite_value_from_row(&self, row: &sqlx::sqlite::SqliteRow, index: usize) -> DataValue;

    /// 将SQL类型映射为FieldType
    ///
    /// # 参数
    /// * `type_info` - SQL类型信息
    ///
    /// # 返回值
    /// * `FieldType` - 映射后的字段类型
    fn map_sql_type_to_field_type(&self, type_info: &impl sqlx::TypeInfo) -> FieldType;
}

/// 实现TransactionConverter trait，提供类型转换功能
impl TransactionConverter for DbTransaction {
    /// 将PostgreSQL行转换为DataSet
    fn convert_postgres_rows_to_dataset(&self, rows: Vec<sqlx::postgres::PgRow>, dataset_id: &str) -> DataSet {
        if rows.is_empty() {
            // 如果没有结果，返回空数据集
            let schema = std::sync::Arc::new(Schema::new(format!("{}", dataset_id), vec![]));
            return DataSet::empty(dataset_id, schema);
        }

        // 构建Schema
        let first_row = &rows[0];
        let column_count = first_row.columns().len();
        let mut fields = Vec::with_capacity(column_count);

        for (_i, column) in first_row.columns().iter().enumerate() {
            let column_name = column.name().to_string();
            let field_type = self.map_sql_type_to_field_type(column.type_info());
            fields.push(Field {
                name: column_name,
                field_type,
                label: "".to_string(),
            });
        }

        let schema = std::sync::Arc::new(Schema::new(format!("{}", dataset_id), fields));
        let mut dataset = DataSet::with_capacity(dataset_id, schema.clone(), rows.len());

        // 转换每一行数据
        for row in rows {
            let mut values = Vec::with_capacity(column_count);
            for i in 0..column_count {
                let value = self.get_postgres_value_from_row(&row, i);
                values.push(value);
            }
            dataset.add_row(Row::new(values));
        }

        dataset
    }

    /// 将MySQL行转换为DataSet
    fn convert_mysql_rows_to_dataset(&self, rows: Vec<sqlx::mysql::MySqlRow>, dataset_id: &str) -> DataSet {
        if rows.is_empty() {
            // 如果没有结果，返回空数据集
            let schema = std::sync::Arc::new(Schema::new(format!("{}", dataset_id), vec![]));
            return DataSet::empty(dataset_id, schema);
        }

        // 构建Schema
        let first_row = &rows[0];
        let column_count = first_row.columns().len();
        let mut fields = Vec::with_capacity(column_count);

        for (_i, column) in first_row.columns().iter().enumerate() {
            let column_name = column.name().to_string();
            let field_type = self.map_sql_type_to_field_type(column.type_info());
            fields.push(Field {
                name: column_name,
                field_type,
                label: "".to_string(),
            });
        }

        let schema = std::sync::Arc::new(Schema::new(format!("{}", dataset_id), fields));
        let mut dataset = DataSet::with_capacity(dataset_id, schema.clone(), rows.len());

        // 转换每一行数据
        for row in rows {
            let mut values = Vec::with_capacity(column_count);
            for i in 0..column_count {
                let value = self.get_mysql_value_from_row(&row, i);
                values.push(value);
            }
            dataset.add_row(Row::new(values));
        }

        dataset
    }

    /// 将SQLite行转换为DataSet
    fn convert_sqlite_rows_to_dataset(&self, rows: Vec<sqlx::sqlite::SqliteRow>, dataset_id: &str) -> DataSet {
        if rows.is_empty() {
            // 如果没有结果，返回空数据集
            let schema = std::sync::Arc::new(Schema::new(format!("{}", dataset_id), vec![]));
            return DataSet::empty(dataset_id, schema);
        }

        // 构建Schema
        let first_row = &rows[0];
        let column_count = first_row.columns().len();
        let mut fields = Vec::with_capacity(column_count);

        for (_i, column) in first_row.columns().iter().enumerate() {
            let column_name = column.name().to_string();
            let field_type = self.map_sql_type_to_field_type(column.type_info());
            fields.push(Field {
                name: column_name,
                field_type,
                label: "".to_string(),
            });
        }

        let schema = std::sync::Arc::new(Schema::new(format!("{}", dataset_id), fields));
        let mut dataset = DataSet::with_capacity(dataset_id, schema.clone(), rows.len());

        // 转换每一行数据
        for row in rows {
            let mut values = Vec::with_capacity(column_count);
            for i in 0..column_count {
                let value = self.get_sqlite_value_from_row(&row, i);
                values.push(value);
            }
            dataset.add_row(Row::new(values));
        }

        dataset
    }

    /// 从PostgreSQL行中获取值
    fn get_postgres_value_from_row(&self, row: &sqlx::postgres::PgRow, index: usize) -> DataValue {
        // 根据不同的列类型进行转换，使用try_get来处理NULL值
        if let Ok(value) = row.try_get::<String, _>(index) {
            DataValue::String(value)
        } else if let Ok(value) = row.try_get::<i64, _>(index) {
            DataValue::Int(value)
        } else if let Ok(value) = row.try_get::<f64, _>(index) {
            DataValue::Float(value)
        } else if let Ok(value) = row.try_get::<bool, _>(index) {
            DataValue::Bool(value)
        } else {
            // 对于其他类型，尝试转换为字符串
            match row.try_get::<String, _>(index) {
                Ok(value) => {
                    // 尝试解析为Decimal
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

    /// 从MySQL行中获取值
    fn get_mysql_value_from_row(&self, row: &sqlx::mysql::MySqlRow, index: usize) -> DataValue {
        // 根据不同的列类型进行转换，使用try_get来处理NULL值
        if let Ok(value) = row.try_get::<String, _>(index) {
            DataValue::String(value)
        } else if let Ok(value) = row.try_get::<i64, _>(index) {
            DataValue::Int(value)
        } else if let Ok(value) = row.try_get::<f64, _>(index) {
            DataValue::Float(value)
        } else if let Ok(value) = row.try_get::<bool, _>(index) {
            DataValue::Bool(value)
        } else {
            // 对于其他类型，尝试转换为字符串
            match row.try_get::<String, _>(index) {
                Ok(value) => {
                    // 尝试解析为Decimal
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

    /// 从SQLite行中获取值
    fn get_sqlite_value_from_row(&self, row: &sqlx::sqlite::SqliteRow, index: usize) -> DataValue {
        // 根据不同的列类型进行转换，使用try_get来处理NULL值
        if let Ok(value) = row.try_get::<String, _>(index) {
            DataValue::String(value)
        } else if let Ok(value) = row.try_get::<i64, _>(index) {
            DataValue::Int(value)
        } else if let Ok(value) = row.try_get::<f64, _>(index) {
            DataValue::Float(value)
        } else if let Ok(value) = row.try_get::<bool, _>(index) {
            DataValue::Bool(value)
        } else {
            // 对于其他类型，尝试转换为字符串
            match row.try_get::<String, _>(index) {
                Ok(value) => {
                    // 尝试解析为Decimal
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

    /// 将SQL类型映射为FieldType
    fn map_sql_type_to_field_type(&self, type_info: &impl sqlx::TypeInfo) -> FieldType {
        // 简化实现，根据类型名称进行映射
        let type_name = format!("{}", type_info);
        let type_name_lower = type_name.to_lowercase();
        
        if type_name_lower.contains("varchar") || type_name_lower.contains("text") || type_name_lower.contains("string") || type_name_lower.contains("char") {
            FieldType::String
        } else if type_name_lower.contains("int") || type_name_lower.contains("bigint") || type_name_lower.contains("smallint") || type_name_lower.contains("tinyint") {
            FieldType::Int
        } else if type_name_lower.contains("float") || type_name_lower.contains("double") || type_name_lower.contains("real") {
            FieldType::Float
        } else if type_name_lower.contains("decimal") || type_name_lower.contains("numeric") || type_name_lower.contains("money") {
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
