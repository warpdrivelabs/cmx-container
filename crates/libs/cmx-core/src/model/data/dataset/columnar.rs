//! ColumnarCodec — DataSet 与「列式包」互转（业务单据小包传输，方案 §5.2 / §10）
//!
//! 列式包结构（键名只出现一次，行是纯值数组，子层按父行 id 挂 childRows）：
//! ```json
//! {
//!   "datasetId": "cv_batch",
//!   "columns": ["id","upper_id","doc_no","total_dr"],
//!   "rows": [ ["20260003", null, "PZ-0007", 1130000.00] ],
//!   "childRows": {
//!     "20260003": { "headers": { "datasetId":"cv_header","columns":[...],"rows":[[...]],"childRows":{...} } }
//!   }
//! }
//! ```
//!
//! 相比行式（每行重复键名）可显著减小体积；前端 `CmxDataSet.fromJSON` 原生识别该结构。
//!
//! 归属说明：本模块放在 dataset 内部，因为 encode 需要遍历 `Row.children`
//! （`pub(super)` 字段，外部 crate 不可见），故列式序列化作为 DataSet 的内在能力。

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::{Map, Value, json};

use super::Schema;
use super::rds::{DataSet, Row};
use crate::model::cell::{DataValue, Field, FieldType};

/// 列式包字段名常量，避免拼写漂移。
mod keys {
    pub const DATASET_ID: &str = "datasetId";
    pub const COLUMNS: &str = "columns";
    pub const ROWS: &str = "rows";
    pub const CHILD_ROWS: &str = "childRows";
    /// 根层 COUNT(*) 结果（仅 DocQuery.count_total=true 路径出现）
    pub const TOTAL: &str = "total";
}

/// 列式编解码器（无状态，方法为关联函数）。
pub struct ColumnarCodec;

impl ColumnarCodec {
    // ─────────────────────────── encode ───────────────────────────

    /// 把 DataSet（含嵌套 children）编码为列式包 `serde_json::Value`。
    ///
    /// - `columns` 取自 `schema.fields` 的顺序（列名只出现一次）。
    /// - `rows` 每项是与 columns 对齐的纯值数组。
    /// - 子数据集按「父行 id」挂到 `childRows[父id][childKey] = 递归列式包`。
    ///   父行 id 取自 schema 中名为 `id` 的列；无该列或无子集时不产出 childRows。
    pub fn encode(ds: &DataSet) -> Value {
        let cols: Vec<String> = ds.schema.fields.iter().map(|f| f.name.clone()).collect();

        // rows：每行按列顺序取值
        let rows: Vec<Value> = ds
            .rows
            .iter()
            .map(|row| Self::encode_row_values(row, &cols, &ds.schema))
            .collect();

        // childRows：父行 id → { childKey → 递归列式包 }
        // 本模块是 dataset 的子模块，可直接访问 Row 的 pub(super) 字段 values/children。
        let id_idx = ds.schema.get_index("id");
        let mut child_rows: Map<String, Value> = Map::new();
        if let Some(id_idx) = id_idx {
            for row in &ds.rows {
                if row.children.is_empty() {
                    continue;
                }
                let parent_key = match row.values.get(id_idx) {
                    Some(dv) => dv_to_key(dv),
                    None => continue,
                };
                let mut per_child: Map<String, Value> = Map::new();
                for (child_key, child_ds) in &row.children {
                    per_child.insert(child_key.clone(), Self::encode(child_ds));
                }
                if !per_child.is_empty() {
                    child_rows.insert(parent_key, Value::Object(per_child));
                }
            }
        }

        let mut obj = Map::new();
        obj.insert(keys::DATASET_ID.into(), json!(ds.id));
        obj.insert(keys::COLUMNS.into(), json!(cols));
        obj.insert(keys::ROWS.into(), Value::Array(rows));
        if !child_rows.is_empty() {
            obj.insert(keys::CHILD_ROWS.into(), Value::Object(child_rows));
        }
        // 根层 COUNT(*) 结果（仅 Some 时输出；老客户端读不到不受影响）
        if let Some(t) = ds.total {
            obj.insert(keys::TOTAL.into(), json!(t));
        }
        Value::Object(obj)
    }

    /// 单行 → 与 columns 对齐的纯值数组。
    fn encode_row_values(row: &Row, cols: &[String], schema: &Schema) -> Value {
        let arr: Vec<Value> = (0..cols.len())
            .map(|i| match row.values.get(i) {
                Some(dv) => serde_json::to_value(dv).unwrap_or(Value::Null),
                None => Value::Null,
            })
            .collect();
        // schema 仅用于潜在扩展（对齐校验），当前按索引取值即可。
        let _ = schema;
        Value::Array(arr)
    }

    // ─────────────────────────── decode ───────────────────────────

    /// 把列式包 `Value` 解码回 DataSet（递归还原 childRows）。
    ///
    /// Schema 由列名推断为 `FieldType::String` 占位——因为列式包本身不带类型，
    /// 值按 JSON 原样转 DataValue（数字→Int/Float，字符串→String，null→Null）。
    /// 若调用方已有权威 Schema（如来自单据定义），可改用 `decode_with_schemas`。
    pub fn decode(pkg: &Value) -> Result<DataSet, String> {
        Self::decode_inner(pkg, None)
    }

    /// 用「datasetId → Arc<Schema>」映射解码，各层用权威 Schema（保留类型精度）。
    pub fn decode_with_schemas(
        pkg: &Value,
        schemas: &HashMap<String, Arc<Schema>>,
    ) -> Result<DataSet, String> {
        Self::decode_inner(pkg, Some(schemas))
    }

    fn decode_inner(
        pkg: &Value,
        schemas: Option<&HashMap<String, Arc<Schema>>>,
    ) -> Result<DataSet, String> {
        let obj = pkg.as_object().ok_or("列式包必须是 JSON 对象")?;
        let dataset_id = obj
            .get(keys::DATASET_ID)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let cols: Vec<String> = obj
            .get(keys::COLUMNS)
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|c| c.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        // Schema：优先用权威映射；否则按列名推断（String 占位）。
        let schema: Arc<Schema> = schemas
            .and_then(|m| m.get(&dataset_id).cloned())
            .unwrap_or_else(|| Arc::new(infer_schema(&dataset_id, &cols)));

        // 行值
        let raw_rows = obj
            .get(keys::ROWS)
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let mut ds = DataSet::with_capacity(dataset_id, schema.clone(), raw_rows.len());
        for raw in &raw_rows {
            let vals = raw.as_array().ok_or("rows 每项必须是数组")?;
            let values: Vec<DataValue> = (0..cols.len())
                .map(|i| {
                    let jv = vals.get(i).cloned().unwrap_or(Value::Null);
                    json_to_data_value(&jv, schema.fields.get(i))
                })
                .collect();
            ds.add_row(Row::new(values));
        }

        // childRows：按父行 id 找到对应行，递归解码挂上
        if let Some(child_rows) = obj.get(keys::CHILD_ROWS).and_then(|v| v.as_object()) {
            let id_idx = ds.schema.get_index("id");
            if let Some(id_idx) = id_idx {
                // 建 父id字符串 → 行下标 的临时索引
                let mut pos_by_id: HashMap<String, usize> = HashMap::new();
                for (idx, row) in ds.rows.iter().enumerate() {
                    if let Some(dv) = row.values.get(id_idx) {
                        pos_by_id.insert(dv_to_key(dv), idx);
                    }
                }
                for (parent_id, per_child) in child_rows {
                    let Some(&pos) = pos_by_id.get(parent_id) else {
                        continue;
                    };
                    let Some(map) = per_child.as_object() else {
                        continue;
                    };
                    for (child_key, child_pkg) in map {
                        let child_ds = Self::decode_inner(child_pkg, schemas)?;
                        ds.rows[pos].add_child(child_key, child_ds);
                    }
                }
            }
        }

        // 根层 COUNT(*) 结果（可选；缺省 → None）
        if let Some(t) = obj.get(keys::TOTAL).and_then(|v| v.as_i64()) {
            ds.total = Some(t);
        }

        Ok(ds)
    }
}

// ─────────────────────────── 小工具 ───────────────────────────

/// DataValue → childRows 的 map key（父行 id 字符串化）。
/// id 一般是 BIGINT/字符串；这里统一转成稳定字符串键。
fn dv_to_key(dv: &DataValue) -> String {
    match dv {
        DataValue::Int(i) => i.to_string(),
        DataValue::String(s) => s.clone(),
        DataValue::ShortStr(s) | DataValue::LongStr(s) => s.to_string(),
        DataValue::Uuid(u) => u.to_string(),
        DataValue::Null | DataValue::NullTyped(_) => String::new(),
        other => serde_json::to_value(other)
            .ok()
            .and_then(|v| match v {
                Value::String(s) => Some(s),
                Value::Number(n) => Some(n.to_string()),
                _ => None,
            })
            .unwrap_or_default(),
    }
}

/// 列名 → 占位 Schema（decode 无权威 Schema 时用，全部 String 类型）。
fn infer_schema(id: &str, cols: &[String]) -> Schema {
    let fields = cols
        .iter()
        .map(|name| Field {
            name: name.clone(),
            field_type: FieldType::String,
            label: String::new(),
        })
        .collect();
    Schema::new_unchecked(id, fields)
}

/// JSON 值 → DataValue（列式包无类型，按 JSON 形态转；有 field 提示时向其类型靠拢）。
fn json_to_data_value(v: &Value, field: Option<&Field>) -> DataValue {
    match v {
        Value::Null => DataValue::Null,
        Value::Bool(b) => DataValue::Bool(*b),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                DataValue::Int(i)
            } else {
                DataValue::Float(n.as_f64().unwrap_or(0.0))
            }
        }
        Value::String(s) => {
            // 有类型提示且为文本类，直接 String；否则也按 String（列式包语义）
            let _ = field;
            DataValue::String(s.clone())
        }
        // 数组/对象：装进 Json 字符串（半结构化）
        other => DataValue::Json(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::cell::{DataValue, FieldType};

    fn schema(id: &str, cols: &[&str]) -> Arc<Schema> {
        Arc::new(Schema::new_unchecked(
            id,
            cols.iter()
                .map(|c| Field {
                    name: (*c).into(),
                    field_type: FieldType::String,
                    label: String::new(),
                })
                .collect(),
        ))
    }

    #[test]
    fn encode_flat_dataset_is_columnar() {
        let s = schema("cv_batch", &["id", "upper_id", "doc_no", "total_dr"]);
        let mut ds = DataSet::empty("cv_batch", s);
        ds.add_row(Row::new(vec![
            DataValue::String("20260003".into()),
            DataValue::Null,
            DataValue::String("PZ-0007".into()),
            DataValue::Float(1130000.0),
        ]));

        let pkg = ColumnarCodec::encode(&ds);
        assert_eq!(pkg["datasetId"], json!("cv_batch"));
        assert_eq!(
            pkg["columns"],
            json!(["id", "upper_id", "doc_no", "total_dr"])
        );
        // 行是纯值数组，键名不重复
        assert_eq!(pkg["rows"][0][0], json!("20260003"));
        assert_eq!(pkg["rows"][0][1], Value::Null);
        assert_eq!(pkg["rows"][0][3], json!(1130000.0));
        // 无子集则无 childRows
        assert!(pkg.get("childRows").is_none());
    }

    #[test]
    fn encode_nested_master_slave() {
        let hs = schema("cv_header", &["id", "upper_id", "total_dr"]);
        let ls = schema("cv_acc_line", &["id", "upper_id", "amount"]);

        let mut header = DataSet::empty("cv_header", hs);
        let mut hrow = Row::new(vec![
            DataValue::String("H1".into()),
            DataValue::Null,
            DataValue::Float(100.0),
        ]);
        let mut lines = DataSet::empty("cv_acc_line", ls);
        lines.add_row(Row::new(vec![
            DataValue::String("L1".into()),
            DataValue::String("H1".into()),
            DataValue::Float(60.0),
        ]));
        lines.add_row(Row::new(vec![
            DataValue::String("L2".into()),
            DataValue::String("H1".into()),
            DataValue::Float(40.0),
        ]));
        hrow.add_child("lines", lines);
        header.add_row(hrow);

        let pkg = ColumnarCodec::encode(&header);
        // childRows 按父行 id "H1" 挂载
        let child = &pkg["childRows"]["H1"]["lines"];
        assert_eq!(child["datasetId"], json!("cv_acc_line"));
        assert_eq!(child["rows"].as_array().unwrap().len(), 2);
        assert_eq!(child["rows"][0][0], json!("L1"));
    }

    #[test]
    fn round_trip_preserves_shape() {
        let hs = schema("cv_header", &["id", "upper_id", "memo"]);
        let ls = schema("cv_line", &["id", "upper_id", "qty"]);
        let mut header = DataSet::empty("cv_header", hs);
        let mut hrow = Row::new(vec![
            DataValue::Int(1001),
            DataValue::Null,
            DataValue::String("备注".into()),
        ]);
        let mut lines = DataSet::empty("cv_line", ls);
        lines.add_row(Row::new(vec![
            DataValue::Int(1),
            DataValue::Int(1001),
            DataValue::Int(7),
        ]));
        hrow.add_child("lines", lines);
        header.add_row(hrow);

        let pkg = ColumnarCodec::encode(&header);
        let back = ColumnarCodec::decode(&pkg).expect("decode ok");
        let pkg2 = ColumnarCodec::encode(&back);
        // encode→decode→encode 幂等：两次列式包结构一致
        assert_eq!(pkg, pkg2);
        // 结构完整性
        assert_eq!(back.id, "cv_header");
        assert_eq!(back.row_count(), 1);
        let child = back.rows[0].get_child("lines").expect("child lines");
        assert_eq!(child.row_count(), 1);
    }

    #[test]
    fn decode_empty_children_ok() {
        let s = schema("t", &["id", "name"]);
        let mut ds = DataSet::empty("t", s);
        ds.add_row(Row::new(vec![
            DataValue::Int(1),
            DataValue::String("a".into()),
        ]));
        let pkg = ColumnarCodec::encode(&ds);
        let back = ColumnarCodec::decode(&pkg).unwrap();
        assert_eq!(back.row_count(), 1);
    }
}
