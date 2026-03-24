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

// serde 序列化/反序列化核心 trait
use serde::{Deserialize, Serialize, Serializer};
// 序列化工具 trait，用于自定义集合序列化
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


}

// ==========================================
// DataSet - 行式数据集
// ==========================================

/// 行式数据集：Schema + 行列表，支持嵌套子 DataSet
///
/// # 结构说明
/// - `id`: 数据集唯一标识
/// - `schema`: 数据结构定义，使用 Arc 实现共享
/// - `rows`: 行数据列表
///
/// # 特点
/// - 支持树形嵌套：每行可包含多个子 DataSet
/// - 序列化友好：可直接转为嵌套 JSON 结构
/// - 内存高效：扁平字段 + Arc 共享 Schema
#[derive(Debug, Clone)]
pub struct DataSet {
    /// 数据集唯一标识
    pub id: String,
    pub schema: Arc<Schema>,
    pub rows: Vec<Row>,
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
        }
    }

    /// 向数据集添加一行数据
    ///
    /// # 参数
    /// - `row`: 要添加的 Row 对象
    pub fn add_row(&mut self, row: Row) {
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
}

/// 类型别名，兼容原有 RowDataSet 命名
/// 注意：新代码建议直接使用 DataSet
pub type RowDataSet = DataSet;

// ==========================================
// 序列化 / 反序列化实现
// ==========================================

/// 序列化时是否跳过 Null 字段（减小 JSON 体积）
/// 设置为 true 可以显著减少输出大小，特别是对于稀疏数据
const SKIP_NULL_IN_SERIALIZE: bool = true;

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
        })
    }
}

/// 从 JSON Value 和 Schema 构建 Row（递归处理子 DataSet）
///
/// # 处理逻辑
/// 1. 解析 JSON 对象
/// 2. 按 schema 字段顺序提取字段值
/// 3. 识别并递归解析子 DataSet（不在 schema 中的字段视为子 DataSet）
///
/// # 参数
/// - `v`: JSON Value，应该是一个对象
/// - `schema`: 用于解析的 Schema 定义
///
/// # 返回值
/// 成功时返回 Row，失败时返回错误信息字符串
fn row_from_value(v: serde_json::Value, schema: &Schema) -> Result<Row, String> {
    let obj = v
        .as_object()
        .ok_or_else(|| "row 应为对象".to_string())?;
    let mut values = Vec::with_capacity(schema.field_count());
    for field in &schema.fields {
        let dv = obj
            .get(&field.name)
            .map(|x: &serde_json::Value| serde_json::from_value::<DataValue>(x.clone()))
            .transpose()
            .map_err(|e| e.to_string())?
            .unwrap_or(DataValue::Null);
        values.push(dv);
    }
    let mut children = HashMap::new();
    for (k, val) in obj.iter() {
        if schema.get_index(k).is_none() {
            let ds =
                serde_json::from_value::<DataSet>(val.clone()).map_err(|e| e.to_string())?;
            children.insert(k.clone(), ds);
        }
    }
    Ok(Row { values, children })
}
