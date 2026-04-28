//! # DataSet 错误类型定义
//!
//! 提供结构化的错误类型，替代现有 `String` 错误和 `panic`，
//! 便于上层业务进行精确的错误匹配和处理。

use thiserror::Error;
use crate::model::cell::FieldType;

/// DataSet 相关错误类型
#[derive(Error, Debug)]
pub enum DataSetError {
    #[error("Schema 相关错误: {reason}")]
    SchemaError { reason: String },
    #[error("字段 '{field_name}' 不存在于数据集 '{dataset_id}' 的 Schema 中")]
    FieldNotFound { field_name: String, dataset_id: String },
    #[error("字段 '{field_name}' 类型不匹配: 期望 {:?}, 实际 {actual}", expected)]
    TypeMismatch { field_name: String, expected: FieldType, actual: String },
    #[error("第 {row_index} 行字段数量不匹配: Schema 期望 {expected_count}, 实际 {actual_count}")]
    RowSchemaMismatch { row_index: usize, expected_count: usize, actual_count: usize },
    #[error("序列化错误: {reason}")]
    SerializationError { reason: String },
    #[error("字段 '{field_name}' 转换为 {target_type} 失败: {reason}")]
    JsonConversionError { field_name: String, target_type: String, reason: String },
}

impl From<DataSetError> for String {
    fn from(err: DataSetError) -> String {
        err.to_string()
    }
}
