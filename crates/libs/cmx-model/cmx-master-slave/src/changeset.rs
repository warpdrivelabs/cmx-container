//! 变更集 —— 与前端 `ChangeSetCollector.export()` 同结构，serde 可序列化。
//!
//! 形状（对齐前端 doc-source.js 注释）：
//! ```json
//! { "cv_header": { "inserted": [ {"id":"t1","upper_id":"...","fields":{...}} ],
//!                  "updated":  [ {"id":"5","fields":{"debit":100},"baseline":"<update_time>"} ],
//!                  "deleted":  ["id1","id2"] } }
//! ```
//! 键是 schema 路径（或前端 root dataset id）。协调器/汇总只认这个中立形状；服务侧
//! `HierService::save` 拿它去落库（doc/dct 现成 saver 就吃这个）。

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashMap;

use crate::agg;
use crate::schema::HierSchema;
use crate::tree::{self, MsTree};

/// 一层的变更。`inserted`/`updated` 是行对象（含 `id` + `fields` + 可选 `upper_id`/`baseline`），
/// `deleted` 是主键值列表。保持 JSON 原样透传，不强解字段——服务侧按各自契约消费。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LayerChanges {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inserted: Vec<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub updated: Vec<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deleted: Vec<Value>,
}

/// 变更集：路径 → 该层变更（`#[serde(flatten)]` 使 JSON 顶层就是 `{path: {...}}`）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChangeSet {
    #[serde(flatten)]
    pub layers: HashMap<String, LayerChanges>,
}

impl ChangeSet {
    /// 从前端 JSON 解析（`ChangeSetCollector.export()` 的产出）。
    pub fn from_json(v: &Value) -> serde_json::Result<Self> {
        serde_json::from_value(v.clone())
    }

    /// 序列化回 JSON（喂给 doc/dct 现成 saver 的 `changes` 字段）。
    pub fn to_json(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

/// 保存结果。承接前端 `refreshBaselines` 所需的 `updatedAt` + temp→real `idMap`。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SaveOutcome {
    pub affected: u64,
    /// temp id → real id（前端 temp 行采用真 id）。
    #[serde(rename = "idMap", default, skip_serializing_if = "Map::is_empty")]
    pub id_map: Map<String, Value>,
    /// 各更新行的新乐观锁基线（前端 `refreshBaselines` 用）。
    #[serde(rename = "updatedAt", default, skip_serializing_if = "Vec::is_empty")]
    pub updated_at: Vec<Value>,
}

/// 取一行的 `fields` 子对象（前端行结构 `{id, fields:{...}}`）；无 `fields` 则视整行为字段。
fn row_fields(row: &Value) -> Map<String, Value> {
    if let Some(f) = row.get("fields").and_then(|v| v.as_object()) {
        f.clone()
    } else if let Some(obj) = row.as_object() {
        obj.clone()
    } else {
        Map::new()
    }
}

/// 写时上卷：对变更集里的 inserted+updated 装树、[`agg::rollup`]、把承接字段写回各行的 `fields`。
///
/// 这是 saver 落库前调用的权威重算入口（对齐统一建模方案「saver 落库前按拓扑重算父层」）。
/// 承接字段既写进行的 `fields`（供落库），也写进行顶层（兼容无 `fields` 包装的行）。
pub fn rollup_changeset(schema: &HierSchema, cs: &mut ChangeSet) -> crate::Result<()> {
    if schema.aggregations.is_empty() {
        return Ok(());
    }
    // 1. 用变更集所有行（inserted+updated）建树
    let mut by_path: HashMap<String, Vec<Map<String, Value>>> = HashMap::new();
    for (path, lc) in &cs.layers {
        let mut rows = Vec::new();
        for r in lc.inserted.iter().chain(lc.updated.iter()) {
            // 行的"平铺视图" = id + upper_id/parent_id + fields 展开（供建树 + 取字段）
            let mut flat = row_fields(r);
            if let Some(id) = r.get("id") {
                flat.insert("id".into(), id.clone());
            }
            // 透传父键（upper_id / parent_id 等可能在行顶层）
            for (k, v) in r.as_object().into_iter().flatten() {
                if k != "fields" && !flat.contains_key(k) {
                    flat.insert(k.clone(), v.clone());
                }
            }
            rows.push(flat);
        }
        by_path.insert(path.clone(), rows);
    }
    let order = schema.layer_order();
    let flat_layers: Vec<(String, String, Option<String>)> = order
        .iter()
        .map(|path| {
            let l = schema.layer(path).unwrap();
            let fk = schema
                .relations
                .iter()
                .find(|r| &r.child == path)
                .map(|r| r.child_key.clone())
                .or_else(|| match &schema.shape {
                    crate::schema::Shape::SelfRef { parent_field } => Some(parent_field.clone()),
                    _ => None,
                });
            (path.clone(), l.pk.clone(), fk)
        })
        .collect();
    let mut t = tree::from_flat(&by_path, &flat_layers);

    // 2. 上卷
    agg::rollup(&mut t, &schema.aggregations)?;

    // 3. 承接字段写回变更集对应行
    let agg_fields: Vec<&String> = schema.aggregations.iter().map(|a| &a.to_field).collect();
    write_back(schema, cs, &t, &agg_fields);
    Ok(())
}

/// 把树里各节点的承接字段写回 changeset 里同 (path,key) 的行（优先 updated，否则 inserted）。
fn write_back(schema: &HierSchema, cs: &mut ChangeSet, t: &MsTree, agg_fields: &[&String]) {
    for node in t.nodes() {
        let updates: Vec<(String, Value)> = agg_fields
            .iter()
            .filter_map(|f| node.row.get(*f).map(|v| ((*f).clone(), v.clone())))
            .collect();
        if updates.is_empty() {
            continue;
        }
        let pk = schema.layer(&node.path).map(|l| l.pk.as_str()).unwrap_or("id");
        if let Some(lc) = cs.layers.get_mut(&node.path)
            && !apply_to_rows(&mut lc.updated, pk, &node.key, &updates)
        {
            apply_to_rows(&mut lc.inserted, pk, &node.key, &updates);
        }
    }
}

/// 在一组行里找 pk == key 的行，把 updates 写进它的 `fields`（无则建）+ 顶层。返回是否命中。
fn apply_to_rows(rows: &mut [Value], pk: &str, key: &str, updates: &[(String, Value)]) -> bool {
    for r in rows.iter_mut() {
        let row_key = r
            .get(pk)
            .or_else(|| r.get("fields").and_then(|f| f.get(pk)))
            .map(tree::value_key)
            .unwrap_or_default();
        if row_key == key {
            let obj = match r.as_object_mut() {
                Some(o) => o,
                None => continue,
            };
            // 写进 fields（若存在），并同步顶层
            if let Some(fields) = obj.get_mut("fields").and_then(|f| f.as_object_mut()) {
                for (k, v) in updates {
                    fields.insert(k.clone(), v.clone());
                }
            }
            for (k, v) in updates {
                obj.insert(k.clone(), v.clone());
            }
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn changeset_json_roundtrip() {
        let v = json!({
            "cv_header": { "inserted": [{"id":"t1","fields":{"voucher_no":"V001"}}] },
            "cv_acc_line": {
                "inserted": [{"id":"t2","upper_id":"t1","fields":{"debit":100}}],
                "deleted": ["9"]
            }
        });
        let cs = ChangeSet::from_json(&v).unwrap();
        assert_eq!(cs.layers.len(), 2);
        assert_eq!(cs.layers["cv_acc_line"].inserted.len(), 1);
        assert_eq!(cs.layers["cv_acc_line"].deleted, vec![json!("9")]);
        // round-trip 保结构
        let back = cs.to_json();
        assert_eq!(back["cv_header"]["inserted"][0]["id"], json!("t1"));
    }

    #[test]
    fn write_time_rollup_writes_back_into_fields() {
        let schema = HierSchema::from_json(&json!({
            "shape": { "kind": "path_tree" },
            "layers": [
                { "path": "head", "table": "cv_header" },
                { "path": "head.items", "table": "cv_line", "child_key": "upper_id" }
            ],
            "relations": [{ "parent": "head", "child": "head.items", "child_key": "upper_id" }],
            "aggregations": [
                { "from": "head.items", "to": "head", "field": "debit", "toField": "total_dr", "agg": "sum" }
            ]
        }))
        .unwrap();
        let mut cs = ChangeSet::from_json(&json!({
            "head":       { "inserted": [{"id":"h1","fields":{}}] },
            "head.items": { "inserted": [
                {"id":"i1","upper_id":"h1","fields":{"debit":100}},
                {"id":"i2","upper_id":"h1","fields":{"debit":50}}
            ]}
        }))
        .unwrap();
        rollup_changeset(&schema, &mut cs).unwrap();
        // head 行的 fields.total_dr 应被写为 150
        let head = &cs.layers["head"].inserted[0];
        assert_eq!(head["fields"]["total_dr"], json!(150));
    }
}
