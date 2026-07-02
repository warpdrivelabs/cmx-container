//! 字典路由编排（复刻 Node `dictRoutes.js` 的路由层逻辑）：
//! 把 schema + repo::search + tree 组合成各端点的最终响应，使 handler 保持薄层。

use serde_json::{json, Value};

use crate::dict::{repo, schema, tree, write};
use crate::error::PortalResult;

/// `POST /api/dict/:dictId/search` —— 单字典检索（hierarchical+treeMode → 树形）。
pub async fn search_endpoint(dict_id: &str, body: &Value) -> PortalResult<Value> {
    let sch = schema::get_schema(dict_id).await?;
    let tree_mode = body.get("treeMode").and_then(|v| v.as_bool()).unwrap_or(false);
    let want_tree = sch.hierarchical && tree_mode;
    let page = body.get("page").and_then(|v| v.as_i64()).unwrap_or(1);
    let page_size = body.get("pageSize").and_then(|v| v.as_i64()).unwrap_or(20);

    let mut q = repo::SearchQuery::from_body(body);
    if want_tree {
        q.page = 1;
        q.page_size = 99999;
    }
    let res = repo::search(dict_id, &q).await?;
    if want_tree {
        Ok(tree::to_tree_result(res.hits, res.total, &sch, page, page_size))
    } else {
        Ok(tree::to_paged_result(res.hits, res.total, page, page_size))
    }
}

/// `GET /api/dict/:dictId/suggest?q=` —— 自动补全。
pub async fn suggest_endpoint(dict_id: &str, q: &str) -> PortalResult<Value> {
    let sch = schema::get_schema(dict_id).await?;
    let q = q.trim();
    if q.is_empty() {
        return Ok(json!({ "suggestions": [] }));
    }
    let label_field = sch.label_field().to_string();
    let query = repo::SearchQuery { q: Some(q.to_string()), page: 1, page_size: 10, ..Default::default() };
    let res = repo::search(dict_id, &query).await?;
    let suggestions: Vec<Value> = res
        .hits
        .into_iter()
        .map(|h| json!({ "text": h.get(&label_field).cloned().unwrap_or(Value::Null), "source": h }))
        .collect();
    Ok(json!({ "suggestions": suggestions }))
}

/// `POST /api/dict/batch-data` —— 多字典内容批量加载。
pub async fn batch_data_endpoint(body: &Value) -> PortalResult<Value> {
    let dict_ids: Vec<String> = if let Some(arr) = body.as_array() {
        arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect()
    } else if let Some(arr) = body.get("dictIds").and_then(|v| v.as_array()) {
        arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect()
    } else {
        vec![]
    };
    let page = body.get("page").and_then(|v| v.as_i64()).unwrap_or(1);
    let page_size = body.get("pageSize").and_then(|v| v.as_i64()).unwrap_or(500);
    let tree_mode = body.get("treeMode").and_then(|v| v.as_bool()).unwrap_or(false);

    let mut results = serde_json::Map::new();
    let mut errors: Vec<Value> = Vec::new();
    for dict_id in dict_ids {
        match schema::get_schema(&dict_id).await {
            Ok(sch) => {
                let want_tree = sch.hierarchical && tree_mode;
                let mut q = repo::SearchQuery::from_body(body);
                q.page = if want_tree { 1 } else { page };
                q.page_size = if want_tree { 99999 } else { page_size };
                match repo::search(&dict_id, &q).await {
                    Ok(res) => {
                        let formatted = if want_tree {
                            tree::to_tree_result(res.hits, res.total, &sch, page, page_size)
                        } else {
                            tree::to_paged_result(res.hits, res.total, page, page_size)
                        };
                        results.insert(dict_id, formatted);
                    }
                    Err(e) => {
                        errors.push(json!({ "dictId": dict_id, "error": e.to_string() }));
                        results.insert(dict_id, json!({ "rows": [], "total": 0, "error": e.to_string() }));
                    }
                }
            }
            Err(e) => {
                errors.push(json!({ "dictId": dict_id, "error": e.to_string() }));
                results.insert(dict_id, json!({ "rows": [], "total": 0, "error": e.to_string() }));
            }
        }
    }
    Ok(json!({ "results": results, "errors": errors }))
}

/// `POST /api/dict/:dictId/entries` —— 写入（rebuild 可选）。
pub async fn upsert_entries_endpoint(dict_id: &str, body: &Value, rebuild: bool) -> PortalResult<Value> {
    let entries: Vec<Value> = if let Some(arr) = body.as_array() {
        arr.clone()
    } else {
        vec![body.clone()]
    };
    write::upsert(dict_id, entries, rebuild).await
}
