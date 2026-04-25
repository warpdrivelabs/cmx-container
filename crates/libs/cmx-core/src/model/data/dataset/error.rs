//! # DataSet 错误类型定义
//!
//! 提供结构化的错误类型，替代现有 `String` 错误和 `panic`，
//! 便于上层业务进行精确的错误匹配和处理。

use std::fmt;

use crate::model::cell::FieldType;

/// DataSet 相关错误类型
#[derive(Debug)]
pub enum DataSetError {
    /// Schema 相关错误（字段名重复、字段缺失等）
    SchemaError {
        /// 错误原因描述
        reason: String,
    },
    /// 字段不存在于 Schema 中
    FieldNotFound {
        /// 字段名称
        field_name: String,
        /// 数据集 ID
        dataset_id: String,
    },
    /// 字段类型不匹配
    TypeMismatch {
        /// 字段名称
        field_name: String,
        /// 期望的类型
        expected: FieldType,
        /// 实际的类型（描述）
        actual: String,
    },
    /// 行数据与 Schema 字段数量不匹配
    RowSchemaMismatch {
        /// 行索引
        row_index: usize,
        /// Schema 期望的字段数量
        expected_count: usize,
        /// 实际的字段数量
        actual_count: usize,
    },
    /// 序列化/反序列化错误
    SerializationError {
        /// 错误原因描述
        reason: String,
    },
    /// JSON 值类型转换失败
    JsonConversionError {
        /// 字段名称
        field_name: String,
        /// 目标类型
        target_type: String,
        /// 错误原因
        reason: String,
    },
}

impl fmt::Display for DataSetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DataSetError::SchemaError { reason } => {
                write!(f, "Schema 错误: {}", reason)
            }
            DataSetError::FieldNotFound { field_name, dataset_id } => {
                write!(f, "字段 '{}' 不存在于数据集 '{}' 的 Schema 中", field_name, dataset_id)
            }
            DataSetError::TypeMismatch { field_name, expected, actual } => {
                write!(f, "字段 '{}' 类型不匹配: 期望 {:?}, 实际 {}", field_name, expected, actual)
            }
            DataSetError::RowSchemaMismatch { row_index, expected_count, actual_count } => {
                write!(f, "第 {} 行字段数量不匹配: Schema 期望 {}, 实际 {}", row_index, expected_count, actual_count)
            }
            DataSetError::SerializationError { reason } => {
                write!(f, "序列化错误: {}", reason)
            }
            DataSetError::JsonConversionError { field_name, target_type, reason } => {
                write!(f, "字段 '{}' 转换为 {} 失败: {}", field_name, target_type, reason)
            }
        }
    }
}

impl std::error::Error for DataSetError {}

impl From<DataSetError> for String {
    fn from(err: DataSetError) -> String {
        err.to_string()
    }
}
