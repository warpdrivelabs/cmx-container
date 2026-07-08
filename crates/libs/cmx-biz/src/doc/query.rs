//! DocQuery —— 通用业务单据加载的**富查询模型**（每层独立：条件/排序/分页/游标）。
//!
//! 设计目标：最灵活的加载指令，推翻旧 `LoadOptions` 的「根层单等值」贫弱设计。
//!
//! - **条件**：每层一棵过滤树 [`Filter`]（AND/OR 组 + 叶子 `列 op 值`），下推到该层 SQL。
//! - **排序**：每层多列 [`OrderBy`]，缺省根层 `[id]`、子层 `[child_key, line_no]`。
//! - **分页**：`limit`/`offset` 或 **keyset 游标** [`Cursor`]（稳定、无漂移，优先游标）。
//! - **深度/懒下钻**：`depth` 限层；`only_parents` 只装指定父的子树（懒下钻端点用）。
//!
//! 安全：所有列名在构建 SQL 前用 `layer.schema.get_index(col)` 白名单校验（[`validate_against`]），
//! 值按列 `data_type` 类型化成 [`DataValue`]，全部走参数绑定（`$N` 占位），零字符串拼接注入面。
//!
//! 出口是 `(sql_text, Vec<DataValue>)`（见 sql_builder），走两驱动都完备的 DataValue 绑定路，
//! **绕开 sea-query Values 的 decimal/array feature 缺口**。

use std::collections::HashMap;

use serde_json::Value;

use cmx_core::model::cell::DataValue;

use super::meta::{ColumnView, LayerView};
use crate::{BizError, Result};

/// 比较算子（叶子条件）。JSON 键 `$eq/$ne/$gt/...`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Eq,
    Ne,
    Gt,
    Gte,
    Lt,
    Lte,
    In,
    NotIn,
    /// SQL LIKE（区分大小写）
    Like,
    /// PG ILIKE（不区分大小写）
    Ilike,
    /// value 里两端 % 包裹的 contains（生成 `%v%`）
    Contains,
    StartsWith,
    EndsWith,
    /// IS NULL / IS NOT NULL（值为 bool：true=IS NULL）
    IsNull,
}

impl Op {
    fn from_key(k: &str) -> Option<Op> {
        Some(match k {
            "$eq" => Op::Eq,
            "$ne" | "$not" => Op::Ne,
            "$gt" => Op::Gt,
            "$gte" => Op::Gte,
            "$lt" => Op::Lt,
            "$lte" => Op::Lte,
            "$in" => Op::In,
            "$notIn" | "$nin" => Op::NotIn,
            "$like" => Op::Like,
            "$ilike" => Op::Ilike,
            "$contains" => Op::Contains,
            "$startsWith" => Op::StartsWith,
            "$endsWith" => Op::EndsWith,
            "$null" | "$isNull" => Op::IsNull,
            _ => return None,
        })
    }
}

/// 一个叶子条件：`列 op 值`。
#[derive(Debug, Clone)]
pub struct Cond {
    pub col: String,
    pub op: Op,
    /// 原始 JSON 值（校验/类型化在 sql_builder 里按列 data_type 做）。
    pub val: Value,
}

/// 过滤树：AND 组、OR 组、或叶子。任意嵌套。
#[derive(Debug, Clone)]
pub enum Filter {
    And(Vec<Filter>),
    Or(Vec<Filter>),
    Leaf(Cond),
}

impl Filter {
    /// 从 JSON 解析过滤树。支持两种写法混用：
    /// - 对象隐式 AND：`{"col1":{"$gt":1}, "col2":{"$eq":"x"}}`
    /// - 列的多算子隐式 AND：`{"amount":{"$gte":10,"$lt":100}}`
    /// - 简写等值：`{"status":"posted"}`（等价 `{"status":{"$eq":"posted"}}`）
    /// - 显式组：`{"$or":[<filter>,...]}` / `{"$and":[<filter>,...]}`
    pub fn from_json(v: &Value) -> Result<Option<Filter>> {
        let Some(obj) = v.as_object() else {
            if v.is_null() {
                return Ok(None);
            }
            return Err(BizError::business("filter 必须是 JSON 对象"));
        };
        if obj.is_empty() {
            return Ok(None);
        }
        let mut ands: Vec<Filter> = Vec::new();
        for (key, val) in obj {
            match key.as_str() {
                "$and" => ands.push(Filter::And(parse_group(val)?)),
                "$or" => ands.push(Filter::Or(parse_group(val)?)),
                col => {
                    // col → 值。值可能是 {op:v,...} 或标量（简写等值）。
                    for leaf in parse_col_conds(col, val)? {
                        ands.push(Filter::Leaf(leaf));
                    }
                }
            }
        }
        Ok(match ands.len() {
            0 => None,
            1 => Some(ands.into_iter().next().unwrap()),
            _ => Some(Filter::And(ands)),
        })
    }

    /// 收集树里用到的全部列名（供 validate_against 白名单校验）。
    fn collect_cols<'a>(&'a self, out: &mut Vec<&'a str>) {
        match self {
            Filter::And(v) | Filter::Or(v) => v.iter().for_each(|f| f.collect_cols(out)),
            Filter::Leaf(c) => out.push(&c.col),
        }
    }
}

fn parse_group(v: &Value) -> Result<Vec<Filter>> {
    let arr = v
        .as_array()
        .ok_or_else(|| BizError::business("$and/$or 必须是数组"))?;
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        if let Some(f) = Filter::from_json(item)? {
            out.push(f);
        }
    }
    Ok(out)
}

/// 解析「某列 → 值」为若干叶子条件。
fn parse_col_conds(col: &str, val: &Value) -> Result<Vec<Cond>> {
    if let Some(op_obj) = val.as_object() {
        // {op:v, op:v, ...} 多算子 AND
        let mut out = Vec::with_capacity(op_obj.len());
        for (opk, opv) in op_obj {
            let op = Op::from_key(opk)
                .ok_or_else(|| BizError::business(format!("不支持的算子 {opk}（列 {col}）")))?;
            out.push(Cond {
                col: col.to_string(),
                op,
                val: opv.clone(),
            });
        }
        Ok(out)
    } else {
        // 标量 → 简写等值
        Ok(vec![Cond {
            col: col.to_string(),
            op: Op::Eq,
            val: val.clone(),
        }])
    }
}

/// 排序项。
#[derive(Debug, Clone)]
pub struct OrderBy {
    pub col: String,
    pub desc: bool,
}

impl OrderBy {
    /// 从字符串解析：`"!col"` = DESC，`"col"` = ASC。
    pub fn parse(s: &str) -> OrderBy {
        if let Some(rest) = s.strip_prefix('!') {
            OrderBy {
                col: rest.to_string(),
                desc: true,
            }
        } else {
            OrderBy {
                col: s.to_string(),
                desc: false,
            }
        }
    }
}

/// keyset 游标：编码「上一页末行的排序列值 + id」。
/// wire 形态：base64(JSON `{"vals":[...],"id":<末行id 的 JSON>}`)。
#[derive(Debug, Clone)]
pub struct Cursor {
    /// 与 order_by 对齐的末值（JSON 原样，类型化在 sql_builder）。
    pub vals: Vec<Value>,
    /// 末行 id（tie-breaker，保证稳定）。
    pub id: Value,
}

impl Cursor {
    pub fn decode(s: &str) -> Result<Cursor> {
        use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
        let bytes = B64
            .decode(s)
            .map_err(|e| BizError::business(format!("游标 base64 解码失败: {e}")))?;
        let v: Value = serde_json::from_slice(&bytes)
            .map_err(|e| BizError::business(format!("游标 JSON 解析失败: {e}")))?;
        let vals = v
            .get("vals")
            .and_then(|x| x.as_array())
            .cloned()
            .unwrap_or_default();
        let id = v.get("id").cloned().unwrap_or(Value::Null);
        Ok(Cursor { vals, id })
    }

    pub fn encode(vals: Vec<Value>, id: Value) -> String {
        use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
        let v = serde_json::json!({ "vals": vals, "id": id });
        B64.encode(serde_json::to_vec(&v).unwrap_or_default())
    }
}

/// 一层的加载指令。
#[derive(Debug, Clone, Default)]
pub struct LayerQuery {
    pub filter: Option<Filter>,
    pub order_by: Vec<OrderBy>,
    pub limit: Option<u64>,
    pub offset: Option<u64>,
    pub cursor: Option<Cursor>,
}

impl LayerQuery {
    fn from_json(v: &Value) -> Result<LayerQuery> {
        let mut lq = LayerQuery::default();
        if let Some(f) = v.get("filter") {
            lq.filter = Filter::from_json(f)?;
        }
        if let Some(ob) = v.get("orderBy") {
            lq.order_by = parse_order_by(ob)?;
        }
        lq.limit = v.get("limit").and_then(|x| x.as_u64());
        lq.offset = v.get("offset").and_then(|x| x.as_u64());
        if let Some(c) = v.get("cursor").and_then(|x| x.as_str())
            && !c.is_empty() {
                lq.cursor = Some(Cursor::decode(c)?);
            }
        Ok(lq)
    }

    /// 校验本层查询：列名必须在该层 schema 内（白名单，防注入）。
    pub fn validate_against(&self, layer: &LayerView) -> Result<()> {
        let mut cols: Vec<&str> = Vec::new();
        if let Some(f) = &self.filter {
            f.collect_cols(&mut cols);
        }
        for ob in &self.order_by {
            cols.push(&ob.col);
        }
        for c in cols {
            if layer.schema.get_index(c).is_none() {
                return Err(BizError::business(format!(
                    "列 {c} 不在层 {} 中（filter/orderBy 非法）",
                    layer.table_name
                )));
            }
        }
        Ok(())
    }
}

fn parse_order_by(v: &Value) -> Result<Vec<OrderBy>> {
    match v {
        Value::String(s) => Ok(vec![OrderBy::parse(s)]),
        Value::Array(arr) => {
            let mut out = Vec::with_capacity(arr.len());
            for item in arr {
                let s = item
                    .as_str()
                    .ok_or_else(|| BizError::business("orderBy 数组项必须是字符串"))?;
                out.push(OrderBy::parse(s));
            }
            Ok(out)
        }
        Value::Null => Ok(Vec::new()),
        _ => Err(BizError::business("orderBy 必须是字符串或字符串数组")),
    }
}

/// 整棵单据树的加载指令。
#[derive(Debug, Clone, Default)]
pub struct DocQuery {
    /// 按层 id 指定该层的 LayerQuery（未指定的层用默认）。
    pub layers: HashMap<String, LayerQuery>,
    /// 装载深度（None=全部层；Some(n)=只装前 n 层）。
    pub depth: Option<usize>,
    /// 是否装同父兄弟表（默认 true）。
    pub include_siblings: bool,
    /// 懒下钻：只装这些父 id 的子树（配合 root_layer_override）。
    pub only_parents: Option<Vec<Value>>,
}

impl DocQuery {
    /// 从 HTTP body JSON 解析。形如：
    /// ```jsonc
    /// { "depth":2, "includeSiblings":true,
    ///   "layers": {
    ///     "cv_batch":  { "filter":{"period_code":"2026"}, "orderBy":["!posting_date"], "limit":50 },
    ///     "cv_acc_line": { "filter":{"local_dr":{"$gt":1000}} }
    ///   } }
    /// ```
    pub fn from_json(v: &Value) -> Result<DocQuery> {
        let mut dq = DocQuery {
            include_siblings: true,
            ..Default::default()
        };
        if v.is_null() {
            return Ok(dq);
        }
        let obj = v
            .as_object()
            .ok_or_else(|| BizError::business("DocQuery 必须是 JSON 对象"))?;
        dq.depth = obj.get("depth").and_then(|x| x.as_u64()).map(|n| n as usize);
        if let Some(b) = obj.get("includeSiblings").and_then(|x| x.as_bool()) {
            dq.include_siblings = b;
        }
        dq.only_parents = obj
            .get("onlyParents")
            .and_then(|x| x.as_array())
            .map(|a| a.to_vec());
        if let Some(layers) = obj.get("layers").and_then(|x| x.as_object()) {
            for (lid, lv) in layers {
                dq.layers.insert(lid.clone(), LayerQuery::from_json(lv)?);
            }
        }
        Ok(dq)
    }

    /// 便捷构造（GET 简单路径：只限根层行数 + 深度）。
    pub fn simple(root_layer_id: &str, limit: Option<u64>, depth: Option<usize>) -> DocQuery {
        let mut dq = DocQuery {
            include_siblings: true,
            depth,
            ..Default::default()
        };
        if limit.is_some() {
            dq.layers.insert(
                root_layer_id.to_string(),
                LayerQuery {
                    limit,
                    ..Default::default()
                },
            );
        }
        dq
    }

    /// 取某层的 LayerQuery（缺省返回默认空指令）。
    pub fn layer(&self, layer_id: &str) -> LayerQuery {
        self.layers.get(layer_id).cloned().unwrap_or_default()
    }

    /// 校验全部层查询的列名合法（对每个在 meta 里的层）。
    pub fn validate(&self, meta: &super::meta::DocMetaView) -> Result<()> {
        for (lid, lq) in &self.layers {
            if let Some(layer) = meta.layer(lid) {
                lq.validate_against(layer)?;
            }
            // 层 id 不在 meta：忽略（前端可能传了多余层，宽松处理）
        }
        Ok(())
    }
}

/// 把 JSON 标量按列的 `data_type` 类型化成 DataValue（供绑定）。
/// 类型不匹配时尽量宽松（数字↔字符串），无法转 → 报错。
pub fn json_to_datavalue(col: &ColumnView, v: &Value) -> Result<DataValue> {
    let dt = col.data_type.to_ascii_uppercase();
    let bad = |want: &str| BizError::business(format!("列 {} 需要 {want}，得到 {v}", col.name));
    Ok(match dt.as_str() {
        "INT" | "INTEGER" | "BIGINT" | "TINYINT" | "SMALLINT" => match v {
            Value::Number(n) if n.is_i64() || n.is_u64() => DataValue::Int(n.as_i64().unwrap()),
            Value::String(s) => DataValue::Int(s.parse().map_err(|_| bad("整数"))?),
            Value::Null => DataValue::Null,
            _ => return Err(bad("整数")),
        },
        "DECIMAL" | "NUMERIC" => match v {
            Value::Number(n) => DataValue::Decimal(
                n.to_string().parse().map_err(|_| bad("十进制数"))?,
            ),
            Value::String(s) => DataValue::Decimal(s.parse().map_err(|_| bad("十进制数"))?),
            Value::Null => DataValue::Null,
            _ => return Err(bad("十进制数")),
        },
        "FLOAT" | "DOUBLE" | "REAL" => match v {
            Value::Number(n) => DataValue::Float(n.as_f64().ok_or_else(|| bad("浮点数"))?),
            Value::String(s) => DataValue::Float(s.parse().map_err(|_| bad("浮点数"))?),
            Value::Null => DataValue::Null,
            _ => return Err(bad("浮点数")),
        },
        "BOOL" | "BOOLEAN" => match v {
            Value::Bool(b) => DataValue::Bool(*b),
            Value::Null => DataValue::Null,
            _ => return Err(bad("布尔")),
        },
        "DATE" => match v {
            Value::String(s) => {
                DataValue::Date(s.parse().map_err(|_| bad("日期 YYYY-MM-DD"))?)
            }
            Value::Null => DataValue::Null,
            _ => return Err(bad("日期字符串")),
        },
        "DATETIME" | "TIMESTAMP" | "TIMESTAMPTZ" => match v {
            Value::String(s) => {
                let dt = chrono::DateTime::parse_from_rfc3339(s)
                    .map(|d| d.with_timezone(&chrono::Utc))
                    .map_err(|_| bad("时间戳 RFC3339"))?;
                DataValue::DateTime(dt)
            }
            Value::Null => DataValue::Null,
            _ => return Err(bad("时间戳字符串")),
        },
        "UUID" => match v {
            Value::String(s) => DataValue::Uuid(s.parse().map_err(|_| bad("UUID"))?),
            Value::Null => DataValue::Null,
            _ => return Err(bad("UUID 字符串")),
        },
        // 文本 / JSON / 其它：字符串
        _ => match v {
            Value::String(s) => DataValue::String(s.clone()),
            Value::Number(n) => DataValue::String(n.to_string()),
            Value::Bool(b) => DataValue::String(b.to_string()),
            Value::Null => DataValue::Null,
            _ => DataValue::String(v.to_string()),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_simple_and_ops() {
        let f = Filter::from_json(&json!({
            "period_code": "2026",
            "local_dr": { "$gt": 1000, "$lt": 5000 }
        }))
        .unwrap()
        .unwrap();
        // 顶层 AND，含 3 个叶子（period_code eq + local_dr gt + local_dr lt）
        let mut cols = Vec::new();
        f.collect_cols(&mut cols);
        cols.sort();
        assert_eq!(cols, vec!["local_dr", "local_dr", "period_code"]);
    }

    #[test]
    fn parse_or_group() {
        let f = Filter::from_json(&json!({
            "$or": [ { "doc_status": "posted" }, { "doc_status": "draft" } ]
        }))
        .unwrap()
        .unwrap();
        assert!(matches!(f, Filter::Or(ref v) if v.len() == 2));
    }

    #[test]
    fn parse_docquery_layers() {
        let dq = DocQuery::from_json(&json!({
            "depth": 2,
            "includeSiblings": false,
            "layers": {
                "cv_batch": { "filter": {"period_code":"2026"}, "orderBy": ["!posting_date","id"], "limit": 50 },
                "cv_acc_line": { "filter": {"local_dr":{"$gte":1000}}, "offset": 20 }
            }
        }))
        .unwrap();
        assert_eq!(dq.depth, Some(2));
        assert!(!dq.include_siblings);
        let batch = dq.layer("cv_batch");
        assert_eq!(batch.limit, Some(50));
        assert_eq!(batch.order_by.len(), 2);
        assert!(batch.order_by[0].desc); // !posting_date
        assert!(!batch.order_by[1].desc);
        assert_eq!(dq.layer("cv_acc_line").offset, Some(20));
        // 未指定层 → 默认
        assert!(dq.layer("cv_header").filter.is_none());
    }

    #[test]
    fn cursor_roundtrip() {
        let enc = Cursor::encode(vec![json!("2026-01-01"), json!(3)], json!(1001));
        let dec = Cursor::decode(&enc).unwrap();
        assert_eq!(dec.vals.len(), 2);
        assert_eq!(dec.id, json!(1001));
    }

    #[test]
    fn order_by_bang_desc() {
        assert!(OrderBy::parse("!amount").desc);
        assert!(!OrderBy::parse("amount").desc);
        assert_eq!(OrderBy::parse("!amount").col, "amount");
    }
}
