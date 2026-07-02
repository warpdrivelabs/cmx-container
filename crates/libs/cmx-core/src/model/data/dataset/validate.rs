//! # 数据集校验模块
//!
//! 为 Row 和 DataSet 提供数据质量校验能力，在框架层面确保数据与 Schema 一致。

use crate::model::cell::{DataValue, FieldType};
use crate::model::data::dataset::Schema;
use crate::model::data::dataset::error::DataSetError;
use crate::model::data::dataset::rds::Row;

/// 数据校验 trait，用于需要外部 Schema 上下文的校验场景
///
/// Row 实现此 trait，因为 Row 自身不持有 Schema 引用
pub trait Validate {
    /// 校验数据是否符合 Schema 定义
    ///
    /// # 返回值
    /// 校验通过返回 `Ok(())`，校验失败返回第一个遇到的错误
    fn validate(&self, schema: &Schema) -> Result<(), DataSetError>;
}

/// 校验 DataValue 是否与 FieldType 兼容（Null 兼容所有类型）
fn check_type_compatible(value: &DataValue, field_type: &FieldType) -> bool {
    match value {
        DataValue::Null | DataValue::NullTyped(_) => true,
        DataValue::Bool(_) => matches!(field_type, FieldType::Bool),
        DataValue::Int(_) => matches!(field_type, FieldType::Int),
        DataValue::Float(_) => matches!(field_type, FieldType::Float),
        DataValue::String(_) | DataValue::ShortStr(_) | DataValue::LongStr(_) => {
            matches!(field_type, FieldType::String | FieldType::Text)
        }
        DataValue::Decimal(_) => matches!(field_type, FieldType::Decimal),
        DataValue::DateTime(_) => matches!(field_type, FieldType::DateTime),
        DataValue::Date(_) => matches!(field_type, FieldType::Date),
        DataValue::Binary(_) => matches!(field_type, FieldType::Binary),
        DataValue::Array(_) => matches!(field_type, FieldType::Array),
        DataValue::Json(_) => matches!(field_type, FieldType::Json | FieldType::Unknown),
        DataValue::Uuid(_) => matches!(field_type, FieldType::Uuid),
    }
}

/// 校验单行数据的字段数量和类型兼容性
///
/// # 参数
/// - `row`: 要校验的行
/// - `schema`: Schema 定义
/// - `row_index`: 行索引（用于错误信息定位），None 表示独立校验无行号上下文
fn validate_row(row: &Row, schema: &Schema, row_index: Option<usize>) -> Result<(), DataSetError> {
    let expected = schema.field_count();
    let actual = row.values.len();
    if expected != actual {
        return Err(DataSetError::RowSchemaMismatch {
            row_index: row_index.unwrap_or(0),
            expected_count: expected,
            actual_count: actual,
        });
    }

    for (i, value) in row.values.iter().enumerate() {
        if let Some(field) = schema.fields.get(i)
            && !check_type_compatible(value, &field.field_type)
        {
            return Err(DataSetError::TypeMismatch {
                field_name: field.name.clone(),
                expected: field.field_type.clone(),
                actual: format!("{:?}", value),
            });
        }
    }

    Ok(())
}

impl Validate for Row {
    /// 校验 Row 数据是否符合 Schema 定义（无行号上下文）
    ///
    /// 检查项：
    /// 1. 字段数量与 Schema 一致
    /// 2. 每个 DataValue 的类型与 FieldType 兼容（Null 兼容所有类型）
    ///
    /// 注意：独立校验 Row 时无法提供行号信息，错误中的 row_index 将为 0。
    /// 如需提供行号上下文，请使用 `validate_with_index()` 方法。
    fn validate(&self, schema: &Schema) -> Result<(), DataSetError> {
        validate_row(self, schema, None)
    }
}

/// Row 的扩展校验方法
impl Row {
    /// 带行号上下文的校验，用于 Row 在 DataSet 中的场景
    ///
    /// 与 `Validate::validate()` 功能相同，但会将 `row_index` 传入错误信息，
    /// 便于定位问题行在数据集中的位置。
    ///
    /// # 参数
    /// - `schema`: Schema 定义
    /// - `row_index`: 行在数据集中的索引位置
    pub fn validate_with_index(
        &self,
        schema: &Schema,
        row_index: usize,
    ) -> Result<(), DataSetError> {
        validate_row(self, schema, Some(row_index))
    }
}

/// DataSet 校验的扩展方法
///
/// DataSet 自身持有 Schema，不需要外部传入，因此使用 inherent method 而非 trait
impl crate::model::data::dataset::DataSet {
    /// 校验 DataSet 中所有行数据是否符合 Schema 定义
    ///
    /// 检查项：
    /// 1. 每行的字段数量与 Schema 一致
    /// 2. 每个 DataValue 的类型与 FieldType 兼容（Null 兼容所有类型）
    ///
    /// 逐一检查每行，遇到第一个错误即返回（错误中包含行号信息）
    pub fn validate_all(&self) -> Result<(), DataSetError> {
        for (i, row) in self.rows.iter().enumerate() {
            validate_row(row, &self.schema, Some(i))?;
        }
        Ok(())
    }
}
