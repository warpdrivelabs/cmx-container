//! 多字典联查（复刻 Node `MultiSearchExecutor`）：父子 join + 分批 + select 裁剪。

use std::collections::HashMap;
use std::pin::Pin;

use serde_json::{Value, json};

use crate::dict::repo::{self, SearchQuery};
use crate::dict::schema::get_schema;
use crate::dict::tree;
use crate::dict::util::field_str;
use crate::error::{PortalError, PortalResult};

const MAX_JOIN_BATCH: usize = 200;

/// 入口：`{ q?, propagateQ?, query: <rootNode> }` → `{ <rootDictId>: <result> }`。
pub async fn execute(body: &Value) -> PortalResult<Value> {
    let root = body.get("query").cloned().unwrap_or(Value::Null);
    let dict_id = root.get("dictId").and_then(|v| v.as_str());
    let Some(dict_id) = dict_id else {
        return Err(PortalError::bad_request("body.query.dictId 不能为空"));
    };
    let global_q = body
        .get("q")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let propagate_q = body
        .get("propagateQ")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let result = exec_node(&root, global_q.as_deref(), propagate_q, None).await?;
    Ok(json!({ dict_id: result }))
}

/// 执行单节点（boxed future：自递归经 join_children → exec_node）。
fn exec_node<'a>(
    node: &'a Value,
    global_q: Option<&'a str>,
    propagate_q: bool,
    parent_join_values: Option<&'a [String]>,
) -> Pin<Box<dyn std::future::Future<Output = PortalResult<Value>> + Send + 'a>> {
    Box::pin(async move {
        let dict_id = node.get("dictId").and_then(|v| v.as_str()).unwrap_or("");
        let schema = get_schema(dict_id).await?;
        let tree_mode = node
            .get("treeMode")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let page = node.get("page").and_then(|v| v.as_i64()).unwrap_or(1);
        let page_size = node.get("pageSize").and_then(|v| v.as_i64()).unwrap_or(50);

        // filters（+ join 注入）
        let mut filters = node
            .get("filters")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();
        if let Some(join_on) = node.get("joinOn")
            && let (Some(pf), Some(vals)) = (
                join_on.get("parentField").and_then(|v| v.as_str()),
                parent_join_values,
            )
            && !vals.is_empty()
        {
            filters.insert(pf.to_string(), json!(vals));
        }

        // effectiveQ：节点 q 优先；否则 propagateQ 时用 globalQ
        let effective_q = if let Some(nq) = node.get("q").and_then(|v| v.as_str()) {
            Some(nq.to_string())
        } else if propagate_q {
            global_q.map(|s| s.to_string())
        } else {
            None
        };

        let fetch_size = if tree_mode { 99999 } else { page_size };
        let query = SearchQuery {
            q: effective_q,
            filters,
            parent_id: node.get("parentId").cloned(),
            ancestor_id: node.get("ancestorId").cloned(),
            page: if tree_mode { 1 } else { page },
            page_size: fetch_size,
            sort_field: node
                .get("sort")
                .and_then(|s| s.get("field"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            sort_desc: node
                .get("sort")
                .and_then(|s| s.get("order"))
                .and_then(|v| v.as_str())
                == Some("desc"),
            ..Default::default()
        };
        let res = repo::search(dict_id, &query).await?;
        let total = res.total;
        let mut hits = res.hits;

        // 子字典 join
        let child_nodes: Vec<Value> = node
            .get("children")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        if !child_nodes.is_empty() {
            hits = join_children(hits, &child_nodes, dict_id, global_q, propagate_q).await?;
        }

        // 格式化
        let mut formatted = if schema.hierarchical && tree_mode {
            tree::to_tree_result(hits, total, &schema, page, page_size)
        } else {
            tree::to_paged_result(hits, total, page, page_size)
        };

        // select 裁剪（非 treeMode）
        if !tree_mode
            && let Some(select) = node.get("select").and_then(|v| v.as_array())
            && !select.is_empty()
        {
            let sel: Vec<String> = select
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect();
            let child_ids: Vec<String> = child_nodes
                .iter()
                .filter_map(|c| {
                    c.get("dictId")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                })
                .collect();
            if let Some(rows) = formatted.get_mut("rows").and_then(|v| v.as_array_mut()) {
                for r in rows.iter_mut() {
                    let mut out = serde_json::Map::new();
                    for f in &sel {
                        if let Some(v) = r.get(f) {
                            out.insert(f.clone(), v.clone());
                        }
                    }
                    for cid in &child_ids {
                        if let Some(v) = r.get(cid) {
                            out.insert(cid.clone(), v.clone());
                        }
                    }
                    *r = Value::Object(out);
                }
            }
        }

        Ok(formatted)
    })
}

async fn join_children(
    parent_hits: Vec<Value>,
    child_nodes: &[Value],
    parent_dict_id: &str,
    global_q: Option<&str>,
    propagate_q: bool,
) -> PortalResult<Vec<Value>> {
    let parent_schema = get_schema(parent_dict_id).await?;
    // 每个子字典独立处理，结果按行 merge
    let mut child_results: Vec<Vec<Value>> = Vec::with_capacity(child_nodes.len());
    for cn in child_nodes {
        let one = join_one_child(&parent_hits, cn, &parent_schema, global_q, propagate_q).await?;
        child_results.push(one);
    }
    // 合并到父行
    Ok(parent_hits
        .into_iter()
        .enumerate()
        .map(|(i, mut row)| {
            for child_rows in &child_results {
                if let (Some(obj), Some(add)) = (
                    row.as_object_mut(),
                    child_rows.get(i).and_then(|v| v.as_object()),
                ) {
                    for (k, v) in add {
                        obj.insert(k.clone(), v.clone());
                    }
                }
            }
            row
        })
        .collect())
}

async fn join_one_child(
    parent_hits: &[Value],
    child_node: &Value,
    parent_schema: &crate::dict::schema::DictSchema,
    global_q: Option<&str>,
    propagate_q: bool,
) -> PortalResult<Vec<Value>> {
    let join_on = child_node.get("joinOn");
    let from_field = join_on
        .and_then(|j| j.get("fromField"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| parent_schema.id_field().to_string());
    let child_dict_id = child_node
        .get("dictId")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let join_values: Vec<String> = parent_hits
        .iter()
        .map(|r| field_str(r, &from_field))
        .filter(|s| !s.is_empty())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();

    if join_values.is_empty() {
        let empty = json!({ "rows": [], "total": 0, "page": 1, "pageSize": 0 });
        return Ok(parent_hits
            .iter()
            .map(|r| {
                let mut row = r.clone();
                if let Some(obj) = row.as_object_mut() {
                    obj.insert(child_dict_id.clone(), empty.clone());
                }
                row
            })
            .collect());
    }

    // 分批
    let mut all_child_rows: Vec<Value> = Vec::new();
    for batch in join_values.chunks(MAX_JOIN_BATCH) {
        let mut cn = child_node.clone();
        if let Some(obj) = cn.as_object_mut() {
            obj.insert("pageSize".to_string(), json!(9999));
        }
        let res = exec_node(&cn, global_q, propagate_q, Some(batch)).await?;
        if let Some(rows) = res.get("rows").and_then(|v| v.as_array()) {
            all_child_rows.extend(rows.iter().cloned());
        }
    }

    // 按 joinOn.parentField 分组
    let child_field = join_on
        .and_then(|j| j.get("parentField"))
        .and_then(|v| v.as_str())
        .unwrap_or("parentId")
        .to_string();
    let mut grouped: HashMap<String, Vec<Value>> = HashMap::new();
    for cr in all_child_rows {
        let key = field_str(&cr, &child_field);
        grouped.entry(key).or_default().push(cr);
    }

    let crows_total = grouped;
    Ok(parent_hits
        .iter()
        .map(|r| {
            let key = field_str(r, &from_field);
            let crows = crows_total.get(&key).cloned().unwrap_or_default();
            let total = crows.len();
            let mut row = r.clone();
            if let Some(obj) = row.as_object_mut() {
                obj.insert(
                    child_dict_id.clone(),
                    json!({ "rows": crows, "total": total }),
                );
            }
            row
        })
        .collect())
}
