//! 结果转换：分页结果 + 平铺→树形（复刻 Node `ResultTransformer`）。

use serde_json::{Value, json};

use crate::dict::schema::DictSchema;

/// 分页结果 `{ rows, total, page, pageSize, totalPages }`。
pub fn to_paged_result(hits: Vec<Value>, total: usize, page: i64, page_size: i64) -> Value {
    let ps = page_size.max(1);
    json!({
        "rows": hits,
        "total": total,
        "page": page,
        "pageSize": page_size,
        "totalPages": (total as i64 + ps - 1) / ps,
    })
}

fn field_str(row: &Value, field: &str) -> String {
    match row.get(field) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Bool(b)) => b.to_string(),
        _ => String::new(),
    }
}

/// 平铺 hits → 树形 `{ treeMode:true, rows:[...children...], total, page, pageSize }`。
///
/// 前提：hits 为全量节点（调用方已用大 pageSize 拉全）。按 parentField 自底向上组装多级子树，
/// 无父或父不在集合内的成为根；剪掉空 children（与 Node `pruneEmpty` 一致）。
pub fn to_tree_result(
    hits: Vec<Value>,
    total: usize,
    schema: &DictSchema,
    page: i64,
    page_size: i64,
) -> Value {
    let id_field = schema.id_field();
    let parent_field = schema.parent_field();

    // 按 level 升序（保持与 Node 同样的稳定遍历顺序）
    let mut sorted = hits;
    sorted.sort_by(|a, b| {
        let la = a.get("level").and_then(|v| v.as_i64()).unwrap_or(0);
        let lb = b.get("level").and_then(|v| v.as_i64()).unwrap_or(0);
        la.cmp(&lb)
    });

    let rows = build_forest(&sorted, id_field, parent_field);
    json!({
        "treeMode": true,
        "rows": rows,
        "total": total,
        "page": page,
        "pageSize": page_size,
    })
}

/// 自底向上组装森林：保证多级子树完整。剪掉空 children。
fn build_forest(sorted: &[Value], id_field: &str, parent_field: &str) -> Value {
    use std::collections::HashMap;
    // children 索引：parent_id → [child_id...]（按原顺序）
    let mut children_of: HashMap<String, Vec<String>> = HashMap::new();
    let mut by_id: HashMap<String, &Value> = HashMap::new();
    let mut all_ids: Vec<String> = Vec::new();
    let mut id_set: std::collections::HashSet<String> = std::collections::HashSet::new();
    for r in sorted {
        let id = field_str(r, id_field);
        by_id.insert(id.clone(), r);
        all_ids.push(id.clone());
        id_set.insert(id);
    }
    for r in sorted {
        let id = field_str(r, id_field);
        let pid = field_str(r, parent_field);
        if !pid.is_empty() && id_set.contains(&pid) {
            children_of.entry(pid).or_default().push(id);
        }
    }
    // 递归组装
    fn assemble(
        id: &str,
        by_id: &HashMap<String, &Value>,
        children_of: &HashMap<String, Vec<String>>,
    ) -> Value {
        let mut node = by_id[id].clone();
        let kids: Vec<Value> = children_of
            .get(id)
            .map(|cids| {
                cids.iter()
                    .map(|cid| assemble(cid, by_id, children_of))
                    .collect()
            })
            .unwrap_or_default();
        if let Some(obj) = node.as_object_mut() {
            if kids.is_empty() {
                obj.remove("children");
            } else {
                obj.insert("children".to_string(), Value::Array(kids));
            }
        }
        node
    }
    // 根 = 无父或父不在集合内
    let roots: Vec<Value> = all_ids
        .iter()
        .filter(|id| {
            let r = by_id[*id];
            let pid = field_str(r, parent_field);
            pid.is_empty() || !id_set.contains(&pid)
        })
        .map(|id| assemble(id, &by_id, &children_of))
        .collect();
    Value::Array(roots)
}
