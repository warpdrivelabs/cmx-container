//! # DataSet 构建器
//!
//! 提供流畅 API 简化 DataSet 的手动构建，解决直接使用 `Row::new(vec![...])` 时
//! 字段顺序易错、代码冗长的问题。
//!
//! ## 使用示例
//!
//! ```ignore
//! let ds = DataSetBuilder::new("orders")
//!     .field("id", FieldType::Int, "ID")
//!     .field("name", FieldType::String, "名称")
//!     .field("price", FieldType::Decimal, "价格")
//!     .row(|r| {
//!         r.set("id", 1i64);
//!         r.set("name", "测试订单");
//!         r.set("price", Decimal::new(9999, 2));
//!     })
//!     .row(|r| {
//!         r.set("id", 2i64);
//!         r.set("name", "另一个订单");
//!     })
//!     .build()
//!     .unwrap();
//! ```

use std::collections::HashMap;
use std::sync::Arc;

use crate::model::cell::{DataValue, Field, FieldType};
use crate::model::data::dataset::rds::Row;
use crate::model::data::dataset::{DataSet, Schema};

/// Row 构建过程中的中间数据，按字段名存储值
struct PendingRow {
    /// 字段名 -> 值 的映射
    values: HashMap<String, DataValue>,
    /// 子数据集映射
    children: HashMap<String, DataSet>,
}

/// Row 级别的构建器，由 DataSetBuilder::row() 闭包回调使用
///
/// 通过 `set()` 方法按字段名赋值，避免手动维护字段顺序
pub struct RowBuilder<'a> {
    /// 所属 DataSetBuilder 的字段定义引用，用于校验字段名合法性
    field_names: &'a [String],
    /// 已设置的字段值
    values: HashMap<String, DataValue>,
    /// 子数据集
    children: HashMap<String, DataSet>,
}

impl<'a> RowBuilder<'a> {
    /// 创建新的 RowBuilder
    fn new(field_names: &'a [String]) -> Self {
        RowBuilder {
            field_names,
            values: HashMap::new(),
            children: HashMap::new(),
        }
    }

    /// 按字段名设置值，自动将 Rust 值转换为 DataValue
    ///
    /// # 参数
    /// - `name`: 字段名称（必须与 DataSetBuilder.field() 中定义的一致）
    /// - `value`: 字段值，任何实现了 `Into<DataValue>` 的类型均可
    ///
    /// # 返回值
    /// 返回 `&mut Self` 以支持链式调用
    ///
    /// # Panics
    /// 当字段名不存在于 Schema 定义中时 panic
    pub fn set<V: Into<DataValue>>(&mut self, name: &str, value: V) -> &mut Self {
        let exists = self.field_names.iter().any(|n| n == name);
        if !exists {
            panic!("DataSetBuilder: 字段名 '{}' 不在 Schema 定义中", name);
        }
        self.values.insert(name.to_string(), value.into());
        self
    }

    /// 添加子数据集到当前行
    ///
    /// # 参数
    /// - `key`: 子数据集的标识键
    /// - `dataset`: 要添加的子 DataSet
    pub fn add_child(&mut self, key: impl AsRef<str>, dataset: DataSet) -> &mut Self {
        self.children.insert(key.as_ref().to_string(), dataset);
        self
    }
}

/// DataSet 构建器，提供流畅 API 简化数据集构建
///
/// # 构建流程
/// 1. `DataSetBuilder::new(id)` 创建构建器
/// 2. `.field()` 链式调用定义 Schema 字段
/// 3. `.row()` 链式调用添加行数据
/// 4. `.build()` 构建最终的 DataSet
pub struct DataSetBuilder {
    /// 数据集 ID
    id: String,
    /// Schema 字段定义（按添加顺序）
    fields: Vec<Field>,
    /// 待构建的行数据
    pending_rows: Vec<PendingRow>,
}

impl DataSetBuilder {
    /// 创建新的 DataSetBuilder 实例
    ///
    /// # 参数
    /// - `id`: 数据集唯一标识
    pub fn new(id: impl Into<String>) -> Self {
        DataSetBuilder {
            id: id.into(),
            fields: Vec::new(),
            pending_rows: Vec::new(),
        }
    }

    /// 添加一个字段定义到 Schema
    ///
    /// # 参数
    /// - `name`: 字段名称
    /// - `field_type`: 字段类型
    /// - `label`: 前端显示标签
    pub fn field(mut self, name: impl Into<String>, field_type: FieldType, label: impl Into<String>) -> Self {
        self.fields.push(Field {
            name: name.into(),
            field_type,
            label: label.into(),
        });
        self
    }

    /// 添加一行数据，通过闭包使用 RowBuilder 按字段名赋值
    ///
    /// # 参数
    /// - `f`: 闭包，接收 `&mut RowBuilder` 参数，在闭包中调用 `set()` 设置字段值
    ///
    /// # 示例
    /// ```ignore
    /// builder.row(|r| {
    ///     r.set("id", 1i64)
    ///      .set("name", "test");
    /// })
    /// ```
    pub fn row<F>(mut self, f: F) -> Self
    where
        F: FnOnce(&mut RowBuilder),
    {
        let field_names: Vec<String> = self.fields.iter().map(|f| f.name.clone()).collect();
        let mut rb = RowBuilder::new(&field_names);
        f(&mut rb);
        self.pending_rows.push(PendingRow {
            values: rb.values,
            children: rb.children,
        });
        self
    }

    /// 基于 Schema 和 Vec<HashMap> 构建 DataSet（不使用闭包的替代 API）
    ///
    /// 适用于数据已经以 HashMap 形式存在的场景
    ///
    /// # 参数
    /// - `id`: 数据集 ID
    /// - `schema`: 已有的 Schema 定义
    /// - `rows_data`: 行数据列表，每行是一个 HashMap<String, DataValue>
    pub fn from_maps(
        id: impl Into<String>,
        schema: Arc<Schema>,
        rows_data: Vec<HashMap<String, DataValue>>,
    ) -> DataSet {
        let mut ds = DataSet::empty(id, schema.clone());
        for data in rows_data {
            let mut values = Vec::with_capacity(schema.field_count());
            for field in &schema.fields {
                let val = data.get(&field.name).cloned().unwrap_or(DataValue::Null);
                values.push(val);
            }
            ds.add_row(Row::new(values));
        }
        ds
    }

    /// 构建最终的 DataSet
    ///
    /// # 返回值
    /// - `Ok(DataSet)`: 构建成功
    /// - `Err(String)`: 构建失败（如 Schema 字段名重复）
    ///
    /// # 注意
    /// 未通过 `set()` 设置的字段将自动填充为 `DataValue::Null`
    pub fn build(self) -> Result<DataSet, String> {
        let schema = Schema::new(&self.id, self.fields)?;
        let schema_arc = Arc::new(schema);
        let mut rows = Vec::with_capacity(self.pending_rows.len());

        for pending in self.pending_rows {
            let mut values = Vec::with_capacity(schema_arc.field_count());
            for field in &schema_arc.fields {
                let val = pending.values.get(&field.name).cloned().unwrap_or(DataValue::Null);
                values.push(val);
            }

            let mut row = Row::new(values);
            for (key, child_ds) in pending.children {
                row.add_child(key, child_ds);
            }
            rows.push(row);
        }

        Ok(DataSet::new(self.id, schema_arc, rows))
    }
}
