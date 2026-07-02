//! 字典写入服务（复刻 Node `DictWriteService`）：
//! upsert 时计算 level/path/pathStr/sortKey/fullText；SCD 停旧启新 deactivate/supersede。

use serde_json::{json, Value};

use crate::dict::repo;
use crate::dict::schema::{get_schema, DictSchema};
use crate::error::{PortalError, PortalResult};

fn today_str() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

fn field_str(row: &Value, field: &str) -> String {
    match row.get(field) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Bool(b)) => b.to_string(),
        _ => String::new(),
    }
}

/// 拼接 fullText（fullTextFields 或 [idField, labelField]）。
fn enrich_full_text(entry: &mut Value, schema: &DictSchema) {
    let fields: Vec<String> = match &schema.full_text_fields {
        Some(f) if !f.is_empty() => f.clone(),
        _ => vec![schema.id_field().to_string(), schema.label_field().to_string()],
    };
    let parts: Vec<String> = fields
        .iter()
        .filter_map(|f| {
            let s = field_str(entry, f);
            if s.is_empty() { None } else { Some(s) }
        })
        .collect();
    if let Some(obj) = entry.as_object_mut() {
        obj.insert("fullText".to_string(), json!(parts.join(" ")));
    }
}

/// upsert 条目（增量推算 level/path，或 rebuild 全量重建）。
pub async fn upsert(dict_id: &str, raw_entries: Vec<Value>, rebuild: bool) -> PortalResult<serde_json::Value> {
    let schema = get_schema(dict_id).await?;
    let id_field = schema.id_field().to_string();
    let parent_field = schema.parent_field().to_string();

    if rebuild {
        let existing = repo::search(dict_id, &big_query()).await?.hits;
        let merged = merge_by_id(existing, raw_entries, &id_field);
        let enriched = build_tree_fields(merged, &id_field, &parent_field, &schema);
        return repo::upsert_entries(dict_id, &enriched).await;
    }

    // 增量：本批 id→entry
    let mut batch_map: std::collections::HashMap<String, Value> = std::collections::HashMap::new();
    for e in &raw_entries {
        batch_map.insert(field_str(e, &id_field), e.clone());
    }
    // 外部父节点（不在本批）
    let parent_ids: Vec<String> = raw_entries
        .iter()
        .filter_map(|e| {
            let p = field_str(e, &parent_field);
            if p.is_empty() { None } else { Some(p) }
        })
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let external_parent_ids: Vec<String> = parent_ids.iter().filter(|id| !batch_map.contains_key(*id)).cloned().collect();

    let mut enriched_map: std::collections::HashMap<String, Option<Value>> = std::collections::HashMap::new();
    if !external_parent_ids.is_empty() {
        let mut q = repo::SearchQuery { page: 1, page_size: external_parent_ids.len() as i64 + 10, ..Default::default() };
        q.filters.insert(id_field.clone(), json!(external_parent_ids));
        let parents = repo::search(dict_id, &q).await?.hits;
        for p in parents {
            enriched_map.insert(field_str(&p, &id_field), Some(p));
        }
    }

    // 迭代式 enrich（带循环保护）
    let enriched = enrich_batch(&raw_entries, &batch_map, &mut enriched_map, &id_field, &parent_field, &schema);
    repo::upsert_entries(dict_id, &enriched).await
}

fn big_query() -> repo::SearchQuery {
    repo::SearchQuery { page: 1, page_size: 99999, include_inactive: true, ..Default::default() }
}

fn merge_by_id(existing: Vec<Value>, incoming: Vec<Value>, id_field: &str) -> Vec<Value> {
    let mut order: Vec<String> = Vec::new();
    let mut map: std::collections::HashMap<String, Value> = std::collections::HashMap::new();
    for r in existing.into_iter().chain(incoming.into_iter()) {
        let k = field_str(&r, id_field);
        if !map.contains_key(&k) {
            order.push(k.clone());
        }
        map.insert(k, r);
    }
    order.into_iter().map(|k| map.remove(&k).unwrap()).collect()
}

/// 拓扑计算 level/path/pathStr/sortKey + fullText（全量）。
fn build_tree_fields(rows: Vec<Value>, id_field: &str, parent_field: &str, schema: &DictSchema) -> Vec<Value> {
    use std::collections::HashMap;
    let mut order: Vec<String> = Vec::with_capacity(rows.len());
    let mut map: HashMap<String, Value> = HashMap::new();
    for r in rows {
        let id = field_str(&r, id_field);
        order.push(id.clone());
        map.insert(id, r);
    }
    let mut visited: std::collections::HashSet<String> = std::collections::HashSet::new();
    // 计算结果缓存：id → (level, path)
    let mut computed: HashMap<String, (i64, Vec<String>)> = HashMap::new();

    fn visit(
        id: &str,
        map: &HashMap<String, Value>,
        id_field: &str,
        parent_field: &str,
        visited: &mut std::collections::HashSet<String>,
        computed: &mut HashMap<String, (i64, Vec<String>)>,
    ) {
        if visited.contains(id) {
            return;
        }
        visited.insert(id.to_string());
        let Some(node) = map.get(id) else { return };
        let pid = field_str(node, parent_field);
        let (level, path) = if !pid.is_empty() && map.contains_key(&pid) {
            visit(&pid, map, id_field, parent_field, visited, computed);
            let (plevel, ppath) = computed.get(&pid).cloned().unwrap_or((1, vec![pid.clone()]));
            let mut path = ppath;
            path.push(id.to_string());
            (plevel + 1, path)
        } else {
            (1, vec![id.to_string()])
        };
        computed.insert(id.to_string(), (level, path));
    }

    for id in &order {
        visit(id, &map, id_field, parent_field, &mut visited, &mut computed);
    }

    order
        .into_iter()
        .map(|id| {
            let mut node = map.remove(&id).unwrap();
            let (level, path) = computed.get(&id).cloned().unwrap_or((1, vec![id.clone()]));
            if let Some(obj) = node.as_object_mut() {
                obj.insert("level".to_string(), json!(level));
                obj.insert("path".to_string(), json!(path));
                obj.insert("pathStr".to_string(), json!(path.join("/")));
                if !obj.contains_key("sortKey") {
                    obj.insert("sortKey".to_string(), json!(id));
                }
            }
            enrich_full_text(&mut node, schema);
            node
        })
        .collect()
}

/// 增量 enrich（父依赖递归 + 循环保护），保持输入顺序。
fn enrich_batch(
    raw_entries: &[Value],
    batch_map: &std::collections::HashMap<String, Value>,
    enriched_map: &mut std::collections::HashMap<String, Option<Value>>,
    id_field: &str,
    parent_field: &str,
    schema: &DictSchema,
) -> Vec<Value> {
    fn enrich(
        id: &str,
        batch_map: &std::collections::HashMap<String, Value>,
        enriched_map: &mut std::collections::HashMap<String, Option<Value>>,
        id_field: &str,
        parent_field: &str,
        schema: &DictSchema,
    ) -> Option<Value> {
        if let Some(v) = enriched_map.get(id) {
            return v.clone();
        }
        let Some(e) = batch_map.get(id) else { return None };
        // 占位防循环
        enriched_map.insert(id.to_string(), None);
        let pid = field_str(e, parent_field);
        let parent = if !pid.is_empty() {
            enrich(&pid, batch_map, enriched_map, id_field, parent_field, schema)
        } else {
            None
        };
        let (level, path) = match &parent {
            Some(p) => {
                let plevel = p.get("level").and_then(|v| v.as_i64()).unwrap_or(1);
                let ppath: Vec<String> = p
                    .get("path")
                    .and_then(|v| v.as_array())
                    .map(|a| a.iter().map(|x| match x { Value::String(s) => s.clone(), o => o.to_string() }).collect())
                    .unwrap_or_else(|| vec![field_str(p, id_field)]);
                let mut path = ppath;
                path.push(id.to_string());
                (plevel + 1, path)
            }
            None => (1, vec![id.to_string()]),
        };
        let mut enriched = e.clone();
        if let Some(obj) = enriched.as_object_mut() {
            obj.insert("level".to_string(), json!(level));
            obj.insert("path".to_string(), json!(path));
            obj.insert("pathStr".to_string(), json!(path.join("/")));
            if !obj.contains_key("sortKey") {
                obj.insert("sortKey".to_string(), json!(id));
            }
        }
        enrich_full_text(&mut enriched, schema);
        enriched_map.insert(id.to_string(), Some(enriched.clone()));
        Some(enriched)
    }

    raw_entries
        .iter()
        .filter_map(|e| {
            let id = field_str(e, id_field);
            enrich(&id, batch_map, enriched_map, id_field, parent_field, schema)
        })
        .collect()
}

// ── SCD 停旧启新 ────────────────────────────────────────────────────────

async fn get_by_key(dict_id: &str, id_field: &str, key: &str) -> PortalResult<Option<Value>> {
    let mut q = repo::SearchQuery { page: 1, page_size: 5, ..Default::default() };
    q.filters.insert(id_field.to_string(), json!(key));
    let hits = repo::search(dict_id, &q).await?.hits;
    Ok(hits.into_iter().find(|r| field_str(r, id_field) == key))
}

/// 停用一个码：status=0 + valid_to。
pub async fn deactivate(dict_id: &str, code: &str, valid_to: Option<&str>, successor_code: Option<&str>) -> PortalResult<serde_json::Value> {
    let schema = get_schema(dict_id).await?;
    let id_field = schema.id_field();
    let row = get_by_key(dict_id, id_field, code)
        .await?
        .ok_or_else(|| PortalError::not_found(format!("字典 \"{dict_id}\" 无此码:{code}")))?;
    let vt = valid_to.map(|s| s.to_string()).unwrap_or_else(today_str);
    let mut patched = row;
    if let Some(obj) = patched.as_object_mut() {
        obj.insert("status".to_string(), json!(0));
        obj.insert("valid_to".to_string(), json!(vt));
        if let Some(sc) = successor_code {
            obj.insert("successor_code".to_string(), json!(sc));
        }
    }
    repo::upsert_entries(dict_id, &[patched.clone()]).await?;
    Ok(json!({
        "ok": true, "dictId": dict_id, "code": code, "status": 0, "valid_to": vt,
        "successor_code": successor_code,
    }))
}

/// 停旧启新：停 oldCode 指向 newCode，newCode 记前驱。
pub async fn supersede(dict_id: &str, old_code: &str, new_code: &str, as_of: Option<&str>, new_entry: Option<&Value>) -> PortalResult<serde_json::Value> {
    let schema = get_schema(dict_id).await?;
    let id_field = schema.id_field().to_string();
    if old_code == new_code {
        return Err(PortalError::bad_request("新旧码不能相同"));
    }
    let old_row = get_by_key(dict_id, &id_field, old_code)
        .await?
        .ok_or_else(|| PortalError::not_found(format!("字典 \"{dict_id}\" 无旧码:{old_code}")))?;
    let cut = as_of.map(|s| s.to_string()).unwrap_or_else(today_str);

    // 新码：已存在则沿用，否则用 newEntry 或克隆旧码业务字段
    let existing_new = get_by_key(dict_id, &id_field, new_code).await?;
    let is_new = existing_new.is_none();
    let mut new_row = match existing_new {
        Some(r) => r,
        None => {
            let mut base = match new_entry {
                Some(v) if v.is_object() => v.clone(),
                _ => clone_business(&old_row, &id_field),
            };
            if let Some(obj) = base.as_object_mut() {
                obj.insert(id_field.clone(), json!(new_code));
                if !obj.contains_key("code") {
                    obj.insert("code".to_string(), json!(new_code));
                }
            }
            base
        }
    };
    if let Some(obj) = new_row.as_object_mut() {
        obj.insert("status".to_string(), json!(1));
        obj.insert("valid_from".to_string(), json!(cut));
        obj.insert("valid_to".to_string(), Value::Null);
        obj.insert("predecessor_code".to_string(), json!(old_code));
    }
    // 运行期新建码补数值代理键
    if is_new {
        let needs_id = new_row.get("id").map(|v| !v.is_number()).unwrap_or(true);
        if needs_id {
            let next = next_surrogate_id(dict_id).await?;
            if let Some(obj) = new_row.as_object_mut() {
                obj.insert("id".to_string(), json!(next));
            }
        }
    }

    // 旧码：停用 + successor
    let mut old_patched = old_row;
    if let Some(obj) = old_patched.as_object_mut() {
        obj.insert("status".to_string(), json!(0));
        obj.insert("valid_to".to_string(), json!(cut));
        obj.insert("successor_code".to_string(), json!(new_code));
    }

    // 新码经 upsert 补树字段；旧码直接 upsertEntries
    upsert(dict_id, vec![strip_tree_aux(&new_row)], false).await?;
    repo::upsert_entries(dict_id, &[old_patched]).await?;
    Ok(json!({ "ok": true, "dictId": dict_id, "oldCode": old_code, "newCode": new_code, "asOf": cut }))
}

async fn next_surrogate_id(dict_id: &str) -> PortalResult<i64> {
    let hits = repo::search(dict_id, &big_query()).await?.hits;
    let mut max = 0i64;
    for r in &hits {
        if let Some(n) = r.get("id").and_then(|v| v.as_i64())
            && n > max {
                max = n;
            }
    }
    Ok(if max == 0 { 1000000 } else { max } + 1)
}

/// 克隆旧码业务字段（去主键/树辅助/生命周期）。
fn clone_business(row: &Value, id_field: &str) -> Value {
    let drop: std::collections::HashSet<&str> = [
        id_field, "id", "level", "path", "pathStr", "level_no", "full_path", "is_leaf", "fullText",
        "valid_from", "valid_to", "predecessor_code", "successor_code",
    ]
    .into_iter()
    .collect();
    let mut out = serde_json::Map::new();
    if let Some(obj) = row.as_object() {
        for (k, v) in obj {
            if !drop.contains(k.as_str()) {
                out.insert(k.clone(), v.clone());
            }
        }
    }
    Value::Object(out)
}

/// 去掉会被 upsert 重算的树辅助字段。
fn strip_tree_aux(row: &Value) -> Value {
    let mut out = row.clone();
    if let Some(obj) = out.as_object_mut() {
        for k in ["level", "path", "pathStr", "fullText"] {
            obj.remove(k);
        }
    }
    out
}
