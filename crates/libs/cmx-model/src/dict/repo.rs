//! 字典条目仓库 + 内存检索引擎（复刻 Node `JsonFileRepo`）。

use serde_json::{json, Value};

use crate::config::data_path;
use crate::dict::schema::{get_schema, try_get_schema, DictSchema};
use crate::error::PortalResult;
use crate::fsutil::{read_json_opt, write_json_atomic};
use crate::util::write_lock;

/// 检索查询参数（与 Node search() 入参对齐；其余字段忽略）。
#[derive(Debug, Clone, Default)]
pub struct SearchQuery {
    pub q: Option<String>,
    pub filters: serde_json::Map<String, Value>,
    pub parent_id: Option<Value>,
    pub ancestor_id: Option<Value>,
    pub page: i64,
    pub page_size: i64,
    pub sort_field: Option<String>,
    pub sort_desc: bool,
    pub include_inactive: bool,
    pub as_of: Option<String>,
}

impl SearchQuery {
    /// 从前端请求体 JSON 解析查询参数。
    pub fn from_body(body: &Value) -> SearchQuery {
        let filters = body
            .get("filters")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();
        let sort = body.get("sort");
        SearchQuery {
            q: body.get("q").and_then(|v| v.as_str()).map(|s| s.to_string()),
            filters,
            parent_id: body.get("parentId").cloned(),
            ancestor_id: body.get("ancestorId").cloned(),
            page: body.get("page").and_then(|v| v.as_i64()).unwrap_or(1),
            page_size: body.get("pageSize").and_then(|v| v.as_i64()).unwrap_or(20),
            sort_field: sort.and_then(|s| s.get("field")).and_then(|v| v.as_str()).map(|s| s.to_string()),
            sort_desc: sort.and_then(|s| s.get("order")).and_then(|v| v.as_str()) == Some("desc"),
            include_inactive: body.get("includeInactive").and_then(|v| v.as_bool()).unwrap_or(false),
            as_of: body.get("asOf").and_then(|v| v.as_str()).map(|s| s.to_string()),
        }
    }
}

/// 检索结果。
pub struct SearchResult {
    pub hits: Vec<Value>,
    pub total: usize,
}

fn entries_dir() -> std::path::PathBuf {
    data_path(["dict", "entries"])
}

/// entries 路径候选：schema 带 DAM → 分层优先，扁平兜底。
fn entries_path_candidates(dict_id: &str, schema: Option<&DictSchema>) -> Vec<std::path::PathBuf> {
    let flat = entries_dir().join(format!("{dict_id}.json"));
    if let Some(s) = schema
        && let (Some(d), Some(a), Some(m)) = (&s.domain, &s.app, &s.module) {
            let dam = entries_dir().join(d).join(a).join(m).join(format!("{dict_id}.json"));
            return vec![dam, flat];
        }
    vec![flat]
}

/// 读取条目（DAM 优先，扁平兜底）。
async fn load_entries(dict_id: &str, schema: Option<&DictSchema>) -> PortalResult<Vec<Value>> {
    for p in entries_path_candidates(dict_id, schema) {
        if let Some(Value::Array(rows)) = read_json_opt(&p).await? {
            return Ok(rows);
        }
    }
    Ok(Vec::new())
}

/// 写回条目（写到候选路径首项 = schema 决定的主路径）。
async fn save_entries(dict_id: &str, schema: Option<&DictSchema>, rows: &[Value]) -> PortalResult<()> {
    let path = entries_path_candidates(dict_id, schema)
        .into_iter()
        .next()
        .unwrap_or_else(|| entries_dir().join(format!("{dict_id}.json")));
    write_json_atomic(&path, &Value::Array(rows.to_vec()), true).await
}

/// 今日 YYYY-MM-DD（本地时间）。
fn current_date_str() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

/// 字段取字符串（数字也转字符串，缺失为空）。
fn field_str(row: &Value, field: &str) -> String {
    match row.get(field) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Bool(b)) => b.to_string(),
        _ => String::new(),
    }
}

// ── 模糊匹配工具（复刻 Node）──────────────────────────────────────────────

/// 简易 Levenshtein（含早停：>2 即返回 3）。
fn levenshtein(text: &str, pattern: &str) -> usize {
    if text.contains(pattern) {
        return 0;
    }
    let tchars: Vec<char> = text.chars().collect();
    let pchars: Vec<char> = pattern.chars().collect();
    if pchars.len() > tchars.len() {
        return pchars.len();
    }
    let mut prev: Vec<usize> = (0..=pchars.len()).collect();
    for (i, tc) in tchars.iter().take(300).enumerate() {
        let mut curr = vec![i + 1];
        for (j, pc) in pchars.iter().enumerate() {
            let v = if tc == pc {
                prev[j]
            } else {
                1 + prev[j].min(prev[j + 1]).min(curr[j])
            };
            curr.push(v);
        }
        if *curr.iter().min().unwrap() > 2 {
            return 3;
        }
        prev = curr;
    }
    prev[pchars.len()]
}

/// 中文字符比例。
fn cjk_ratio(s: &str) -> f64 {
    let total = s.chars().count();
    if total == 0 {
        return 0.0;
    }
    let cjk = s
        .chars()
        .filter(|c| matches!(*c, '\u{4e00}'..='\u{9fff}' | '\u{3400}'..='\u{4dbf}'))
        .count();
    cjk as f64 / total as f64
}

/// pattern 是否为 text 的子序列。
fn is_subsequence(pattern: &str, text: &str) -> bool {
    if pattern.is_empty() {
        return true;
    }
    let pchars: Vec<char> = pattern.chars().collect();
    let mut pi = 0;
    for tc in text.chars() {
        if pi < pchars.len() && tc == pchars[pi] {
            pi += 1;
        }
    }
    pi == pchars.len()
}

/// 中文模糊匹配：包含 → 去掉一个字后子序列匹配。
fn fuzzy_match_cjk(text: &str, term: &str) -> bool {
    if text.contains(term) {
        return true;
    }
    let chars: Vec<char> = term.chars().collect();
    if chars.len() < 2 {
        return false;
    }
    for i in 0..chars.len() {
        let sub: String = chars.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, c)| *c).collect();
        if is_subsequence(&sub, text) {
            return true;
        }
    }
    false
}

/// 行的全文：优先 fullText 字段，否则拼接所有 string/number 值。
fn row_full_text(row: &Value) -> String {
    if let Some(ft) = row.get("fullText").and_then(|v| v.as_str()) {
        return ft.to_lowercase();
    }
    let mut parts = Vec::new();
    if let Some(obj) = row.as_object() {
        for v in obj.values() {
            match v {
                Value::String(s) => parts.push(s.clone()),
                Value::Number(n) => parts.push(n.to_string()),
                _ => {}
            }
        }
    }
    parts.join(" ").to_lowercase()
}

/// 内存检索引擎（复刻 Node `JsonFileRepo.search`）。
pub async fn search(dict_id: &str, query: &SearchQuery) -> PortalResult<SearchResult> {
    let schema = try_get_schema(dict_id).await?;
    let rows = load_entries(dict_id, schema.as_ref()).await?;
    let mut result: Vec<Value> = rows;

    let id_field = schema.as_ref().map(|s| s.id_field().to_string()).unwrap_or_else(|| "id".to_string());
    let hierarchical = schema.as_ref().map(|s| s.hierarchical).unwrap_or(false);
    let parent_field = schema.as_ref().map(|s| s.parent_field().to_string()).unwrap_or_else(|| "parentId".to_string());

    // 显式按主键查找 → 放宽有效性过滤
    let explicit_key_lookup = query
        .filters
        .get(&id_field)
        .map(|v| !v.is_null() && v != "")
        .unwrap_or(false);

    // 树形：直接子节点
    if hierarchical {
        if let Some(pid) = &query.parent_id {
            if pid.is_null() {
                result.retain(|r| {
                    let v = field_str(r, &parent_field);
                    v.is_empty()
                });
            } else {
                let want = value_to_string(pid);
                result.retain(|r| field_str(r, &parent_field) == want);
            }
        }
        // 树形：所有后代（path 含 ancestorId）
        if let Some(anc) = &query.ancestor_id {
            let want = value_to_string(anc);
            result.retain(|r| {
                r.get("path")
                    .and_then(|p| p.as_array())
                    .map(|arr| arr.iter().any(|x| value_to_string(x) == want))
                    .unwrap_or(false)
            });
        }
    }

    // 属性过滤
    for (field, value) in &query.filters {
        if value.is_null() || value == "" {
            continue;
        }
        if let Some(arr) = value.as_array() {
            let set: std::collections::HashSet<String> = arr.iter().map(value_to_string).collect();
            result.retain(|r| set.contains(&field_str(r, field)));
        } else if let Some(s) = value.as_str() {
            if s.contains('*') {
                // 通配符 → 正则（^...$，* → .*）
                let escaped = regex_escape_glob(s);
                if let Ok(re) = regex::Regex::new(&format!("(?i)^{escaped}$")) {
                    result.retain(|r| re.is_match(&field_str(r, field)));
                }
            } else {
                let want = s.to_string();
                result.retain(|r| field_str(r, field) == want);
            }
        } else {
            let want = value_to_string(value);
            result.retain(|r| field_str(r, field) == want);
        }
    }

    // 有效性过滤（停旧启新）
    if !explicit_key_lookup {
        if let Some(as_of) = &query.as_of {
            let t = as_of.clone();
            result.retain(|r| {
                let vf = r.get("valid_from").and_then(|v| v.as_str());
                let vt = r.get("valid_to").and_then(|v| v.as_str());
                vf.map(|x| x <= t.as_str()).unwrap_or(true) && vt.map(|x| t.as_str() < x).unwrap_or(true)
            });
        } else if !query.include_inactive {
            let today = current_date_str();
            result.retain(|r| {
                let active = match r.get("status") {
                    None | Some(Value::Null) => true,
                    Some(v) => v.as_i64() == Some(1) || v.as_str() == Some("1"),
                };
                let vt = r.get("valid_to").and_then(|v| v.as_str());
                active && vt.map(|x| today.as_str() < x).unwrap_or(true)
            });
        }
    }

    // 全文 / 模糊
    if let Some(q) = &query.q {
        let term = q.trim().to_lowercase();
        if !term.is_empty() {
            result.retain(|r| {
                let ft = row_full_text(r);
                if ft.contains(&term) {
                    return true;
                }
                if cjk_ratio(&term) >= 0.5 {
                    return fuzzy_match_cjk(&ft, &term);
                }
                let tlen = term.chars().count();
                if (2..=8).contains(&tlen) {
                    return levenshtein(&ft, &term) <= 2;
                }
                false
            });
        }
    }

    // 排序
    let sf = query.sort_field.clone().unwrap_or_else(|| "sortKey".to_string());
    let desc = query.sort_desc;
    result.sort_by(|a, b| {
        let av = field_str(a, &sf);
        let bv = field_str(b, &sf);
        if desc {
            bv.cmp(&av)
        } else {
            av.cmp(&bv)
        }
    });

    let total = result.len();
    let page = query.page.max(1);
    let from = ((page - 1) * query.page_size).max(0) as usize;
    let hits: Vec<Value> = result.into_iter().skip(from).take(query.page_size.max(0) as usize).collect();
    Ok(SearchResult { hits, total })
}

/// 值转字符串（数组/对象保持 JSON）。
fn value_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// 转义通配符串为正则：保护正则元字符，`*` → `.*`。
fn regex_escape_glob(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '*' => out.push_str(".*"),
            '.' | '+' | '^' | '$' | '{' | '}' | '(' | ')' | '|' | '[' | ']' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

// ── 写入 ────────────────────────────────────────────────────────────────

/// upsert 条目（按 idField 合并）。
pub async fn upsert_entries(dict_id: &str, entries: &[Value]) -> PortalResult<serde_json::Value> {
    let _guard = write_lock().lock().await;
    let schema = try_get_schema(dict_id).await?;
    let id_field = schema.as_ref().map(|s| s.id_field().to_string()).unwrap_or_else(|| "id".to_string());
    let mut rows = load_entries(dict_id, schema.as_ref()).await?;
    // 用 idField 建索引
    let mut index: std::collections::HashMap<String, usize> = rows
        .iter()
        .enumerate()
        .map(|(i, r)| (field_str(r, &id_field), i))
        .collect();
    for e in entries {
        let key = field_str(e, &id_field);
        if let Some(&i) = index.get(&key) {
            rows[i] = e.clone();
        } else {
            index.insert(key, rows.len());
            rows.push(e.clone());
        }
    }
    save_entries(dict_id, schema.as_ref(), &rows).await?;
    Ok(json!({ "count": entries.len() }))
}

/// 删除单条目。
pub async fn delete_entry(dict_id: &str, id: &str) -> PortalResult<serde_json::Value> {
    let _guard = write_lock().lock().await;
    let schema = try_get_schema(dict_id).await?;
    let id_field = schema.as_ref().map(|s| s.id_field().to_string()).unwrap_or_else(|| "id".to_string());
    let mut rows = load_entries(dict_id, schema.as_ref()).await?;
    rows.retain(|r| field_str(r, &id_field) != id);
    save_entries(dict_id, schema.as_ref(), &rows).await?;
    Ok(json!({ "ok": true }))
}

/// 清空字典条目。
pub async fn clear_entries(dict_id: &str) -> PortalResult<serde_json::Value> {
    let _guard = write_lock().lock().await;
    let schema = try_get_schema(dict_id).await?;
    save_entries(dict_id, schema.as_ref(), &[]).await?;
    Ok(json!({ "ok": true }))
}

/// 取 schema（供 write/multi 用，未注册 404）。
pub async fn require_schema(dict_id: &str) -> PortalResult<DictSchema> {
    get_schema(dict_id).await
}
