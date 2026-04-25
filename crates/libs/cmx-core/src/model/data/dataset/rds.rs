//! # 行式数据集 (Row DataSet) - 设计思路
//!
//! ## 设计理念
//!
//! 本模块实现了基于行式存储的数据集结构，用于支持 ERP 系统中的主子/树形数据结构。
//! 核心设计目标是：在保持内存效率的同时，提供灵活的嵌套数据结构和便捷的序列化能力。
//!
//! ## 核心组件
//!
//! ### 1. Row (行数据)
//! - **存储优化**：使用 `Vec<DataValue>` 数组存储扁平字段，减少内存分配开销
//! - **嵌套支持**：通过 `HashMap<String, DataSet>` 支持子数据集，实现主子/树形结构
//! - **访问模式**：支持按索引和按字段名（需 Schema）两种方式访问数据
//!
//! ### 2. DataSet (行式数据集)
//! - **结构设计**：Schema + 行列表，Schema 定义结构，rows 存储实际数据
//! - **共享优化**：Schema 使用 `Arc` 包裹，多个 DataSet 可共享同一 Schema 定义
//! - **类型别名**：提供 `RowDataSet` 别名以兼容原有命名
//!
//! ## 序列化策略
//!
//! - **跳过 Null 值**：序列化时自动跳过 Null 字段，减小 JSON 体积
//! - **携带 Schema**：序列化时包含完整 Schema 信息，支持反序列化重建
//! - **嵌套处理**：递归处理子 DataSet，输出树形 JSON 结构
//!
//! ## 使用场景
//!
//! - 订单头行结构（订单头 + 订单行列表）
//! - BOM 结构（产品 + 子件列表）
//! - 主从表数据展示
//! - 需要嵌套 JSON 输出的 API 响应
//!
//! ## 性能考虑
//!
//! - 字段访问 O(1)：Schema 维护 name->index 映射
//! - 内存紧凑：扁平字段用 Vec，避免 HashMap 开销
//! - 零拷贝潜力：未来可结合 Cow 等类型进一步优化

use std::collections::HashMap;
use std::convert::TryFrom;
use std::sync::Arc;

use base64::Engine;
use serde::{Deserialize, Serialize, Serializer};
use serde::ser::{SerializeMap, SerializeSeq};

use crate::model::cell::DataValue;
use crate::model::data::dataset::Schema;


// ==========================================
// Row - 支持树形结构
// ==========================================

/// 行数据，支持扁平字段与嵌套子数据集（主子/树形）
///
/// # 结构说明
/// - `values`: 核心扁平数据，使用数组存储以节省内存
/// - `children`: 子数据集映射，Key 为 DataSetID，Value 为嵌套的 DataSet
///
/// # 使用示例
/// ```ignore
/// let mut row = Row::new(vec![1.into(), "test".into()]);
/// row.add_child("details", child_dataset);
/// ```
#[derive(Debug, Clone)]
pub struct Row {
    /// 核心扁平数据 (数组存储，省内存)
    pub(super) values: Vec<DataValue>,
    /// 子数据集 Key: DataSetID，Value: 嵌套的 DataSet
    pub(super) children: HashMap<String, DataSet>,
}

impl Row {
    /// 创建新的 Row 实例
    ///
    /// # 参数
    /// - `values`: DataValue 向量，包含该行的所有字段值
    pub fn new(values: Vec<DataValue>) -> Self {
        Row {
            values,
            children: HashMap::new(),
        }
    }

    /// 预分配容量，适合已知行宽时减少扩容
    ///
    /// # 参数
    /// - `value_count`: 预分配的字段数量
    pub fn with_capacity(value_count: usize) -> Self {
        Row {
            values: Vec::with_capacity(value_count),
            children: HashMap::new(),
        }
    }

    /// 添加子数据集到当前行
    ///
    /// # 参数
    /// - `key`: 子数据集的标识键
    /// - `dataset`: 要添加的子 DataSet
    pub fn add_child(&mut self, key: impl AsRef<str>, dataset: DataSet) {
        self.children.insert(key.as_ref().to_string(), dataset);
    }

    /// 获取子数据集的不可变引用
    ///
    /// # 参数
    /// - `key`: 子数据集的标识键
    ///
    /// # 返回值
    /// 如果存在则返回子数据集的引用，否则返回 None
    pub fn get_child(&self, key: &str) -> Option<&DataSet> {
        self.children.get(key)
    }

    /// 获取子数据集的可变引用
    ///
    /// # 参数
    /// - `key`: 子数据集的标识键
    ///
    /// # 返回值
    /// 如果存在则返回子数据集的可变引用，否则返回 None
    pub fn get_child_mut(&mut self, key: &str) -> Option<&mut DataSet> {
        self.children.get_mut(key)
    }

    /// 按索引获取字段的不可变引用
    ///
    /// # 参数
    /// - `index`: 字段在 values 数组中的索引
    ///
    /// # 返回值
    /// 如果索引有效则返回 DataValue 的引用，否则返回 None
    pub fn get(&self, index: usize) -> Option<&DataValue> {
        self.values.get(index)
    }

    /// 按索引获取字段的可变引用
    ///
    /// # 参数
    /// - `index`: 字段在 values 数组中的索引
    ///
    /// # 返回值
    /// 如果索引有效则返回 DataValue 的可变引用，否则返回 None
    pub fn get_mut(&mut self, index: usize) -> Option<&mut DataValue> {
        self.values.get_mut(index)
    }

    /// 按字段名获取值（需传入所属 Schema）
    ///
    /// # 参数
    /// - `schema`: 字段所属的 Schema，用于将字段名转换为索引
    /// - `name`: 字段名称
    ///
    /// # 返回值
    /// 如果字段存在则返回 DataValue 的引用，否则返回 None
    pub fn get_by_name(&self, schema: &Schema, name: &str) -> Option<&DataValue> {
        schema.get_index(name).and_then(|i| self.values.get(i))
    }

    /// 按字段名获取值并转换为指定类型（需传入所属 Schema）
    ///
    /// # 参数
    /// - `schema`: 字段所属的 Schema，用于将字段名转换为索引
    /// - `name`: 字段名称
    ///
    /// # 返回值
    /// 如果字段存在且能转换为目标类型 T 则返回 Some(T)，否则返回 None
    ///
    /// # 支持的类型
    /// - i32, i64, f64, bool, String, Decimal
    /// - DateTime<Utc>, NaiveDate, Uuid
    /// - Vec<u8> (Binary), Vec<DataValue> (Array)
    pub fn get_by_name_as<T: TryFrom<DataValue, Error = String>>(
        &self,
        schema: &Schema,
        name: &str,
    ) -> Option<T> {
        self.get_by_name(schema, name)
            .and_then(|v| T::try_from(v.clone()).ok())
    }

    /// 检查字段名是否存在于 Schema 中
    ///
    /// 用于区分"字段不存在"和"字段值为 Null"两种情况
    pub fn field_exists(schema: &Schema, name: &str) -> bool {
        schema.get_index(name).is_some()
    }

    /// 按字段名获取值，返回结构化结果以区分"字段不存在"和"值为 Null"
    ///
    /// # 返回值
    /// - `Ok(Some(&DataValue))` — 字段存在且有值
    /// - `Ok(None)` — 字段存在但值为 Null
    /// - `Err(DataSetError)` — 字段不存在
    pub fn get_by_name_checked(
        &self,
        schema: &Schema,
        name: &str,
    ) -> Result<Option<&DataValue>, crate::model::data::dataset::error::DataSetError> {
        use crate::model::data::dataset::error::DataSetError;
        match schema.get_index(name) {
            Some(i) => match self.values.get(i) {
                Some(DataValue::Null) => Ok(None),
                Some(v) => Ok(Some(v)),
                None => Ok(None),
            },
            None => Err(DataSetError::FieldNotFound {
                field_name: name.to_string(),
                dataset_id: String::new(),
            }),
        }
    }

    /// 根据 Schema 创建一个预填充 Null 的空行
    ///
    /// 所有字段值初始化为 DataValue::Null，后续可通过 `set_by_name` 设置具体值
    ///
    /// # 参数
    /// - `schema`: 行所属的 Schema，用于确定字段数量
    pub fn from_schema(schema: &Schema) -> Self {
        let values = vec![DataValue::Null; schema.field_count()];
        Row {
            values,
            children: HashMap::new(),
        }
    }

    /// 按字段名设置值（需传入所属 Schema）
    ///
    /// # 参数
    /// - `schema`: 行所属的 Schema，用于将字段名转换为索引
    /// - `name`: 字段名称
    /// - `value`: 要设置的值（任何可转换为 DataValue 的类型）
    ///
    /// # 返回值
    /// 设置成功返回 Ok(())，字段名不存在返回 Err
    pub fn set_by_name<V: Into<DataValue>>(
        &mut self,
        schema: &Schema,
        name: &str,
        value: V,
    ) -> Result<(), String> {
        match schema.get_index(name) {
            Some(i) => {
                if i < self.values.len() {
                    self.values[i] = value.into();
                    Ok(())
                } else {
                    Err(format!(
                        "索引 {} 超出 values 长度 {}（字段 '{}'）",
                        i,
                        self.values.len(),
                        name
                    ))
                }
            }
            None => Err(format!("字段 '{}' 不在 Schema 中", name)),
        }
    }

    /// 校验 Row 的字段数量是否与 Schema 匹配
    ///
    /// # 参数
    /// - `schema`: 行所属的 Schema
    ///
    /// # 返回值
    /// 匹配返回 Ok(())，不匹配返回详细的错误信息
    pub fn validate_schema(&self, schema: &Schema) -> Result<(), String> {
        let expected = schema.field_count();
        let actual = self.values.len();
        if expected != actual {
            Err(format!(
                "Row 字段数量不匹配: Schema 期望 {}, 实际 {}",
                expected, actual
            ))
        } else {
            Ok(())
        }
    }

    /// 从 serde_json::Value 构建 Row（按 Schema 映射字段）
    ///
    /// 根据 Schema 字段顺序提取值，缺失字段填充 Null。
    /// 不在 Schema 中的 JSON key 被视为子 DataSet。
    ///
    /// # 参数
    /// - `value`: JSON Value，应为对象类型
    /// - `schema`: Schema 定义，用于字段映射
    pub fn from_json_value(value: serde_json::Value, schema: &Schema) -> Result<Self, super::error::DataSetError> {
        use super::error::DataSetError;
        let mut obj = match value {
            serde_json::Value::Object(map) => map,
            _ => return Err(DataSetError::SerializationError {
                reason: "Row::from_json_value: JSON 值应为对象".to_string(),
            }),
        };
        let mut values = Vec::with_capacity(schema.field_count());
        for field in &schema.fields {
            let dv = match obj.remove(&field.name) {
                Some(v) => serde_json::from_value::<DataValue>(v)
                    .map_err(|e| DataSetError::JsonConversionError {
                        field_name: field.name.clone(),
                        target_type: format!("{:?}", field.field_type),
                        reason: e.to_string(),
                    })?,
                None => DataValue::Null,
            };
            values.push(dv);
        }
        let mut children = HashMap::new();
        for (k, val) in obj {
            if schema.get_index(&k).is_none() {
                let ds = serde_json::from_value::<DataSet>(val)
                    .map_err(|e| DataSetError::SerializationError {
                        reason: format!("子 DataSet '{}' 解析失败: {}", k, e),
                    })?;
                children.insert(k, ds);
            }
        }
        Ok(Row { values, children })
    }

    /// 将 Row 转为 serde_json::Value（需要 Schema 提供字段名）
    ///
    /// 输出格式为 JSON 对象，字段名来自 Schema 定义。
    /// 子 DataSet 也一并序列化。
    pub fn to_json_value(&self, schema: &Schema) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        for (i, field) in schema.fields.iter().enumerate() {
            if let Some(value) = self.values.get(i) {
                map.insert(field.name.clone(), serde_json::to_value(value).unwrap_or(serde_json::Value::Null));
            }
        }
        for (key, child_ds) in &self.children {
            map.insert(key.clone(), serde_json::to_value(child_ds).unwrap_or(serde_json::Value::Null));
        }
        serde_json::Value::Object(map)
    }

    /// 格式化为带字段名的调试字符串（需要 Schema 上下文）
    ///
    /// 当前 Row 的标准 Debug 输出只显示 `values: [Int(1), String("test")]`，
    /// 无法直观看出字段名。此方法结合 Schema 输出可读格式：
    /// `Row { id: Int(1), name: String("test") }`
    pub fn debug_with_schema(&self, schema: &Schema) -> String {
        let pairs: Vec<String> = self
            .values
            .iter()
            .enumerate()
            .filter_map(|(i, v)| {
                schema
                    .fields
                    .get(i)
                    .map(|f| format!("{}: {:?}", f.name, v))
            })
            .collect();
        let children_info = if self.children.is_empty() {
            String::new()
        } else {
            format!(", children: {:?}", self.children.keys().collect::<Vec<_>>())
        };
        format!("Row {{ {}{} }}", pairs.join(", "), children_info)
    }

}

// ==========================================
// DataSet - 行式数据集
// ==========================================

/// 行式数据集：Schema + 行列表，支持嵌套子 DataSet
///
/// # 结构说明
/// - `id`: 数据集唯一标识
/// - `schema`: 数据结构定义，使用 Arc 实现共享
/// - `rows`: 行数据列表（已持久化/查询得到的数据）
/// - `inserted`: 新增数据池（待插入的行）
/// - `updated`: 更新数据池（待更新的行）
/// - `deleted`: 删除数据池（待删除的行）
///
/// # 特点
/// - 支持树形嵌套：每行可包含多个子 DataSet
/// - 序列化友好：可直接转为嵌套 JSON 结构
/// - 内存高效：扁平字段 + Arc 共享 Schema
/// - 变更追踪：通过 inserted/updated/deleted 池记录数据变更
#[derive(Debug, Clone)]
pub struct DataSet {
    /// 数据集唯一标识
    pub id: String,
    /// 数据结构定义
    pub schema: Arc<Schema>,
    /// 当前行数据（已持久化的 / 查询得到的数据）
    pub rows: Vec<Row>,
    /// 新增数据池（待插入的行）
    pub inserted: Vec<Row>,
    /// 更新数据池（待更新的行）
    pub updated: Vec<Row>,
    /// 删除数据池（待删除的行）
    pub deleted: Vec<Row>,
}

impl DataSet {
    /// 创建新的 DataSet 实例
    ///
    /// # 参数
    /// - `id`: 数据集唯一标识
    /// - `schema`: 数据结构定义（Arc 包裹）
    /// - `rows`: 初始行数据列表
    pub fn new(id: impl Into<String>, schema: Arc<Schema>, rows: Vec<Row>) -> Self {
        DataSet {
            id: id.into(),
            schema,
            rows,
            inserted: Vec::new(),
            updated: Vec::new(),
            deleted: Vec::new(),
        }
    }

    /// 创建一个空的 DataSet 实例
    ///
    /// # 参数
    /// - `id`: 数据集唯一标识
    /// - `schema`: 数据结构定义（Arc 包裹）
    pub fn empty(id: impl Into<String>, schema: Arc<Schema>) -> Self {
        DataSet {
            id: id.into(),
            schema,
            rows: Vec::new(),
            inserted: Vec::new(),
            updated: Vec::new(),
            deleted: Vec::new(),
        }
    }

    /// 预分配行容量的 DataSet
    ///
    /// # 参数
    /// - `id`: 数据集唯一标识
    /// - `schema`: 数据结构定义（Arc 包裹）
    /// - `cap`: 预分配的行数容量
    pub fn with_capacity(id: impl Into<String>, schema: Arc<Schema>, cap: usize) -> Self {
        DataSet {
            id: id.into(),
            schema,
            rows: Vec::with_capacity(cap),
            inserted: Vec::new(),
            updated: Vec::new(),
            deleted: Vec::new(),
        }
    }

    /// 向数据集添加一行数据
    ///
    /// 在 debug 模式下会校验行字段数量是否与 Schema 匹配，不匹配会 panic
    ///
    /// # 参数
    /// - `row`: 要添加的 Row 对象
    pub fn add_row(&mut self, row: Row) {
        debug_assert!(
            row.values.len() == self.schema.field_count(),
            "add_row: Row 字段数量 {} 与 Schema 字段数量 {} 不匹配 (dataset: {})",
            row.values.len(),
            self.schema.field_count(),
            self.id
        );
        self.rows.push(row);
    }

    /// 获取数据集的行数
    #[inline]
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// 检查数据集是否为空
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// 获取行的迭代器
    pub fn iter(&self) -> std::slice::Iter<'_, Row> {
        self.rows.iter()
    }

    /// 从 serde_json::Value 数组构建 DataSet
    ///
    /// 适用于前端提交 JSON 数组需要转为 DataSet 的场景
    ///
    /// # 参数
    /// - `id`: 数据集唯一标识
    /// - `schema`: Schema 定义（Arc 共享）
    /// - `values`: JSON Value 数组，每个元素应为 JSON 对象
    pub fn from_json_array(
        id: impl Into<String>,
        schema: Arc<Schema>,
        values: Vec<serde_json::Value>,
    ) -> Result<Self, super::error::DataSetError> {
        let rows = values
            .into_iter()
            .map(|v| Row::from_json_value(v, &schema))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(DataSet::new(id, schema, rows))
    }
}

/// 类型别名，兼容原有 RowDataSet 命名
/// 注意：新代码建议直接使用 DataSet
pub type RowDataSet = DataSet;

// ==========================================
// 序列化 / 反序列化实现
// ==========================================

/// 序列化时是否跳过 Null 字段（减小 JSON 体积）
/// 设置为 true 可以显著减少输出大小，特别是对于稀疏数据
const SKIP_NULL_IN_SERIALIZE: bool = false;

/// DataSet 的 Serialize 实现
///
/// 序列化结构：
/// ```json
/// {
///   "id": "数据集 ID",
///   "schema": Schema 对象，
///   "rows": [行数组]
/// }
/// ```
impl Serialize for DataSet {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut st = serializer.serialize_struct("DataSet", 3)?;
        // 序列化数据集 ID
        st.serialize_field("id", &self.id)?;
        // 序列化 Schema 定义
        st.serialize_field("schema", self.schema.as_ref())?;
        // 序列化行数据（包装为 RowsRef 以携带 schema 信息）
        st.serialize_field(
            "rows",
            &RowsRef {
                rows: &self.rows,
                schema: &self.schema,
            },
        )?;
        st.end()
    }
}

/// 行数组的序列化包装器（携带 schema 以按名输出字段）
///
/// # 作用
/// 将 rows 与 schema 绑定，使得序列化时可以按字段名而非索引输出
struct RowsRef<'a> {
    rows: &'a [Row],
    schema: &'a Schema,
}

impl<'a> Serialize for RowsRef<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(self.rows.len()))?;
        for row in self.rows {
            seq.serialize_element(&RowRef {
                row,
                schema: self.schema,
            })?;
        }
        seq.end()
    }
}

/// Row 的序列化包装器，将 Row 与 Schema 绑定以便按字段名序列化
///
/// # 序列化行为
/// - 按 schema 字段顺序输出所有非 Null 值
/// - 输出所有子 DataSet（children）
struct RowRef<'a> {
    row: &'a Row,
    schema: &'a Schema,
}

impl<'a> Serialize for RowRef<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let len = self.row.values.len() + self.row.children.len();
        let mut map = serializer.serialize_map(Some(len))?;

        // 序列化扁平字段：按 schema 定义的字段名输出
        for (i, value) in self.row.values.iter().enumerate() {
            if let Some(field) = self.schema.fields.get(i) {
                if SKIP_NULL_IN_SERIALIZE && matches!(value, DataValue::Null) {
                    continue;
                }
                map.serialize_entry(&field.name, value)?;
            }
        }
        // 序列化子数据集：直接输出 children 中的所有 DataSet
        for (key, child_dataset) in &self.row.children {
            map.serialize_entry(key, child_dataset)?;
        }
        map.end()
    }
}

/// 反序列化用中间结构（rows 先以 Value 形式读入再转 Row）
///
/// # 设计原因
/// 由于 Row 的结构依赖于 Schema，需要先读取 schema 字段，
/// 然后根据 schema 将 JSON Value 转换为 Row
#[derive(Deserialize)]
struct DataSetDe {
    id: String,
    schema: Schema,
    rows: Vec<serde_json::Value>,
}

/// DataSet 的 Deserialize 实现
///
/// # 反序列化流程
/// 1. 读取 id、schema 和 rows（原始 JSON Value）
/// 2. 根据 schema 构建 Arc
/// 3. 遍历 rows，调用 `row_from_value` 将每个 JSON Value 转为 Row
/// 4. 构建并返回完整的 DataSet
impl<'de> Deserialize<'de> for DataSet {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let de = DataSetDe::deserialize(deserializer)?;
        let schema = Arc::new(de.schema);
        let rows = de
            .rows
            .into_iter()
            .map(|v| row_from_value(v, &schema))
            .collect::<Result<Vec<_>, _>>()
            .map_err(serde::de::Error::custom)?;
        Ok(DataSet {
            id: de.id,
            schema,
            rows,
            inserted: Vec::new(),
            updated: Vec::new(),
            deleted: Vec::new(),
        })
    }
}

/// 从 JSON Value 和 Schema 构建 Row（递归处理子 DataSet）
///
/// # 处理逻辑
/// 1. 解析 JSON 对象
/// 2. 按 schema 字段顺序提取字段值（利用 FieldType 精确推断类型）
/// 3. 识别并递归解析子 DataSet（不在 schema 中的字段视为子 DataSet）
///
/// # 与独立 DataValue 反序列化的区别
/// 独立 DataValue 反序列化依赖启发式规则（如 "2024-01-01" → Date），
/// 而 Schema 感知反序列化根据 field_type 精确映射，消除类型推断歧义。
///
/// # 参数
/// - `v`: JSON Value，应该是一个对象
/// - `schema`: 用于解析的 Schema 定义
fn row_from_value(v: serde_json::Value, schema: &Schema) -> Result<Row, String> {
    let mut obj = match v {
        serde_json::Value::Object(map) => map,
        _ => return Err("row 应为对象".to_string()),
    };
    let mut values = Vec::with_capacity(schema.field_count());
    for field in &schema.fields {
        let dv = match obj.remove(&field.name) {
            Some(json_val) => json_value_to_typed_data(json_val, &field.field_type)?,
            None => DataValue::Null,
        };
        values.push(dv);
    }
    let mut children = HashMap::new();
    for (k, val) in obj {
        if schema.get_index(&k).is_none() {
            if let Ok(ds) = serde_json::from_value::<DataSet>(val) {
                children.insert(k, ds);
            }
        }
    }
    Ok(Row { values, children })
}

/// 根据 FieldType 将 JSON Value 精确转换为 DataValue
///
/// 解决独立 DataValue 反序列化中的类型推断歧义：
/// - 字符串 "2024-01-01" 当 field_type=String 时保持为 String，不误判为 Date
/// - 字符串 "550e8400-..." 当 field_type=String 时保持为 String，不误判为 Uuid
/// - 数组 [1,2,3] 当 field_type=Array 时为 Array，当 field_type=Binary 时为 Binary
///
/// # 参数
/// - `v`: JSON Value
/// - `field_type`: Schema 中定义的字段类型
fn json_value_to_typed_data(
    v: serde_json::Value,
    field_type: &crate::model::cell::FieldType,
) -> Result<DataValue, String> {
    use crate::model::cell::FieldType;

    if v.is_null() {
        return Ok(DataValue::Null);
    }

    match field_type {
        FieldType::Int => match v {
            serde_json::Value::Number(n) => n
                .as_i64()
                .map(DataValue::Int)
                .ok_or_else(|| format!("无法将 {:?} 转为 Int", n)),
            _ => serde_json::from_value::<DataValue>(v).map_err(|e| e.to_string()),
        },
        FieldType::Float => match v {
            serde_json::Value::Number(n) => n
                .as_f64()
                .map(DataValue::Float)
                .ok_or_else(|| format!("无法将 {:?} 转为 Float", n)),
            _ => serde_json::from_value::<DataValue>(v).map_err(|e| e.to_string()),
        },
        FieldType::Bool => match v {
            serde_json::Value::Bool(b) => Ok(DataValue::Bool(b)),
            _ => serde_json::from_value::<DataValue>(v).map_err(|e| e.to_string()),
        },
        FieldType::String | FieldType::Text => match v {
            serde_json::Value::String(s) => Ok(DataValue::String(s)),
            _ => serde_json::from_value::<DataValue>(v).map_err(|e| e.to_string()),
        },
        FieldType::Decimal => match v {
            serde_json::Value::String(s) => s
                .parse::<rust_decimal::Decimal>()
                .map(DataValue::Decimal)
                .map_err(|e| e.to_string()),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Ok(DataValue::Decimal(rust_decimal::Decimal::from(i)))
                } else if let Some(f) = n.as_f64() {
                    rust_decimal::Decimal::try_from(f)
                        .map(DataValue::Decimal)
                        .map_err(|e| e.to_string())
                } else {
                    Err(format!("无法将 {:?} 转为 Decimal", n))
                }
            }
            _ => serde_json::from_value::<DataValue>(v).map_err(|e| e.to_string()),
        },
        FieldType::DateTime => match v {
            serde_json::Value::String(s) => chrono::DateTime::parse_from_rfc3339(&s)
                .map(|dt| DataValue::DateTime(dt.with_timezone(&chrono::Utc)))
                .map_err(|e| format!("DateTime 解析失败 '{}': {}", s, e)),
            _ => serde_json::from_value::<DataValue>(v).map_err(|e| e.to_string()),
        },
        FieldType::Date => match v {
            serde_json::Value::String(s) => chrono::NaiveDate::parse_from_str(&s, "%Y-%m-%d")
                .map(DataValue::Date)
                .map_err(|e| format!("Date 解析失败 '{}': {}", s, e)),
            _ => serde_json::from_value::<DataValue>(v).map_err(|e| e.to_string()),
        },
        FieldType::Binary => match v {
            serde_json::Value::String(s) => {
                if let Some(encoded) = s.strip_prefix("B64:") {
                    base64::engine::general_purpose::STANDARD
                        .decode(encoded)
                        .map(DataValue::Binary)
                        .map_err(|e| format!("Binary base64 解码失败: {}", e))
                } else {
                    base64::engine::general_purpose::STANDARD
                        .decode(&s)
                        .map(DataValue::Binary)
                        .map_err(|e| format!("Binary base64 解码失败: {}", e))
                }
            }
            serde_json::Value::Array(arr) => {
                let bytes: Vec<u8> = arr
                    .iter()
                    .filter_map(|v| v.as_u64().map(|n| n as u8))
                    .collect();
                Ok(DataValue::Binary(bytes))
            }
            _ => serde_json::from_value::<DataValue>(v).map_err(|e| e.to_string()),
        },
        FieldType::Array => match v {
            serde_json::Value::Array(arr) => {
                let items: Result<Vec<DataValue>, _> = arr
                    .into_iter()
                    .map(|item| serde_json::from_value::<DataValue>(item).map_err(|e| e.to_string()))
                    .collect();
                Ok(DataValue::Array(items?))
            }
            _ => serde_json::from_value::<DataValue>(v).map_err(|e| e.to_string()),
        },
        FieldType::Json => match v {
            serde_json::Value::String(s) => Ok(DataValue::Json(s)),
            obj @ serde_json::Value::Object(_) => {
                Ok(DataValue::Json(serde_json::to_string(&obj).unwrap_or_default()))
            }
            arr @ serde_json::Value::Array(_) => {
                Ok(DataValue::Json(serde_json::to_string(&arr).unwrap_or_default()))
            }
            _ => serde_json::from_value::<DataValue>(v).map_err(|e| e.to_string()),
        },
        FieldType::Uuid => match v {
            serde_json::Value::String(s) => uuid::Uuid::parse_str(&s)
                .map(DataValue::Uuid)
                .map_err(|e| format!("Uuid 解析失败 '{}': {}", s, e)),
            _ => serde_json::from_value::<DataValue>(v).map_err(|e| e.to_string()),
        },
        FieldType::Unknown => {
            serde_json::from_value::<DataValue>(v).map_err(|e| e.to_string())
        }
    }
}
