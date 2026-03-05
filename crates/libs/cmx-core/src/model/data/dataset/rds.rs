//! 行式数据集 (Row DataSet)：DataSet 与 Row 及序列化/反序列化

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize, Serializer};
use serde::ser::{SerializeMap, SerializeSeq};

use crate::model::cell::DataValue;
use crate::model::data::dataset::Schema;


// ==========================================
// Row - 支持树形结构
// ==========================================

/// 行数据，支持扁平字段与嵌套子数据集（主子/树形）
#[derive(Debug, Clone)]
pub struct Row {
    /// 核心扁平数据 (数组存储，省内存)
    pub(super) values: Vec<DataValue>,
    /// 子数据集 Key: DataSetID，Value: 嵌套的 DataSet
    pub(super) children: HashMap<String, DataSet>,
}

impl Row {
    pub fn new(values: Vec<DataValue>) -> Self {
        Row {
            values,
            children: HashMap::new(),
        }
    }

    /// 预分配容量，适合已知行宽时减少扩容
    pub fn with_capacity(value_count: usize) -> Self {
        Row {
            values: Vec::with_capacity(value_count),
            children: HashMap::new(),
        }
    }

    /// 添加子数据集
    pub fn add_child(&mut self, key: impl AsRef<str>, dataset: DataSet) {
        self.children.insert(key.as_ref().to_string(), dataset);
    }

    /// 获取子数据集
    pub fn get_child(&self, key: &str) -> Option<&DataSet> {
        self.children.get(key)
    }

    /// 获取子数据集可变引用
    pub fn get_child_mut(&mut self, key: &str) -> Option<&mut DataSet> {
        self.children.get_mut(key)
    }

    pub fn get(&self, index: usize) -> Option<&DataValue> {
        self.values.get(index)
    }

    /// 按索引获取可变引用
    pub fn get_mut(&mut self, index: usize) -> Option<&mut DataValue> {
        self.values.get_mut(index)
    }

    /// 按字段名获取值（需传入所属 Schema）
    pub fn get_by_name(&self, schema: &Schema, name: &str) -> Option<&DataValue> {
        schema.get_index(name).and_then(|i| self.values.get(i))
    }
}

// ==========================================
// DataSet
// ==========================================

/// 行式数据集：Schema + 行列表，支持嵌套子 DataSet
#[derive(Debug, Clone)]
pub struct DataSet {
    /// 数据集唯一标识
    pub id: String,
    pub schema: Arc<Schema>,
    pub rows: Vec<Row>,
}

impl DataSet {
    pub fn new(id: impl Into<String>, schema: Arc<Schema>, rows: Vec<Row>) -> Self {
        DataSet {
            id: id.into(),
            schema,
            rows,
        }
    }

    pub fn empty(id: impl Into<String>, schema: Arc<Schema>) -> Self {
        DataSet {
            id: id.into(),
            schema,
            rows: Vec::new(),
        }
    }

    /// 预分配行容量
    pub fn with_capacity(id: impl Into<String>, schema: Arc<Schema>, cap: usize) -> Self {
        DataSet {
            id: id.into(),
            schema,
            rows: Vec::with_capacity(cap),
        }
    }

    pub fn add_row(&mut self, row: Row) {
        self.rows.push(row);
    }

    #[inline]
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Row> {
        self.rows.iter()
    }
}

/// 类型别名，兼容原有 RowDataSet 命名
pub type RowDataSet = DataSet;

// ==========================================
// 序列化 / 反序列化
// ==========================================

/// 序列化时是否跳过 Null 字段（减小 JSON 体积）
const SKIP_NULL_IN_SERIALIZE: bool = true;

impl Serialize for DataSet {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut st = serializer.serialize_struct("DataSet", 3)?;
        st.serialize_field("id", &self.id)?;
        st.serialize_field("schema", self.schema.as_ref())?;
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

/// 行数组的序列化包装（携带 schema 以按名输出字段）
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

/// 将 Row 与 Schema 绑定以便按字段名序列化
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
        for (i, value) in self.row.values.iter().enumerate() {
            if let Some(field) = self.schema.fields.get(i) {
                if SKIP_NULL_IN_SERIALIZE && matches!(value, DataValue::Null) {
                    continue;
                }
                map.serialize_entry(&field.name, value)?;
            }
        }
        for (key, child_dataset) in &self.row.children {
            map.serialize_entry(key, child_dataset)?;
        }
        map.end()
    }
}

/// 反序列化用中间结构（rows 先以 Value 形式读入再转 Row）
#[derive(Deserialize)]
struct DataSetDe {
    id: String,
    schema: Schema,
    rows: Vec<serde_json::Value>,
}

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
