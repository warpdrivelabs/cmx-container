//! 原生页面存储：索引 JSON（`native-pages/index.json`）+ 源文件（`native-pages/sources/<relPath>`，js/html）。
//!
//! 复刻 Node `lib/nativePagesStore.js`：list（分页，索引项原样）/ get / batch / save。
//! relPath 防穿越：非绝对、无 `..`、扩展名须 js/mjs/html/htm。

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::config::data_path;
use crate::error::{PortalError, PortalResult};
use crate::fsutil::{read_json_opt, read_text_opt, write_json_atomic, write_text_atomic};
use crate::util::{is_safe_id, is_safe_segment, write_lock};

/// 保存入参。
#[derive(Debug, Clone, Deserialize)]
pub struct NativePageInput {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub details: Option<String>,
    #[serde(default, rename = "sourceType")]
    pub source_type: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default, rename = "relPath")]
    pub rel_path: Option<String>,
}

/// 完整页面（含源码）。
#[derive(Debug, Clone, Serialize)]
pub struct NativePageFull {
    pub id: String,
    pub name: String,
    pub details: String,
    #[serde(rename = "sourceType")]
    pub source_type: String,
    #[serde(rename = "relPath")]
    pub rel_path: String,
    pub source: String,
}

fn index_path() -> std::path::PathBuf {
    data_path(["native-pages", "index.json"])
}

fn source_abs(rel: &str) -> std::path::PathBuf {
    let mut p = data_path(["native-pages", "sources"]);
    for seg in rel.split('/') {
        p.push(seg);
    }
    p
}

fn validate_id(id: &str) -> PortalResult<String> {
    let s = id.trim();
    if s.is_empty() || !is_safe_id(s) {
        return Err(PortalError::bad_request("native page id 仅允许字母、数字、._-，长度 1-128"));
    }
    for part in s.split('.') {
        if part.is_empty() || !is_safe_segment(part) {
            return Err(PortalError::bad_request("native page id 段非法"));
        }
    }
    Ok(s.to_string())
}

fn source_type_from_rel(rel: &str) -> String {
    let lower = rel.to_lowercase();
    if lower.ends_with(".js") || lower.ends_with(".mjs") {
        "js".to_string()
    } else if lower.ends_with(".html") || lower.ends_with(".htm") {
        "html".to_string()
    } else {
        String::new()
    }
}

fn validate_source_type(st: &str) -> PortalResult<String> {
    let t = st.trim().to_lowercase();
    if t == "js" || t == "html" {
        Ok(t)
    } else {
        Err(PortalError::bad_request("sourceType 仅支持 js/html"))
    }
}

fn default_rel_path(id: &str, source_type: &str) -> String {
    let base = id.split('.').collect::<Vec<_>>().join("/");
    let ext = if source_type == "html" { "html" } else { "js" };
    format!("{base}.{ext}")
}

/// 校验并规范化 relPath（posix），防穿越。
fn validate_rel_path(rel: &str) -> PortalResult<String> {
    let raw = rel.trim();
    if raw.is_empty() || raw.starts_with('/') || raw.contains('\\') {
        return Err(PortalError::bad_request("relPath 非法"));
    }
    // 简易 posix 规范：拒绝任何 `..` 段
    if raw.split('/').any(|seg| seg == "..") {
        return Err(PortalError::bad_request("relPath 非法"));
    }
    if source_type_from_rel(raw).is_empty() {
        return Err(PortalError::bad_request("源文件仅支持 .js/.mjs/.html/.htm"));
    }
    Ok(raw.to_string())
}

async fn load_index() -> PortalResult<Vec<serde_json::Value>> {
    match read_json_opt(&index_path()).await? {
        Some(doc) => Ok(doc.get("pages").and_then(|p| p.as_array()).cloned().unwrap_or_default()),
        None => Ok(Vec::new()),
    }
}

async fn save_index(pages: &[serde_json::Value]) -> PortalResult<()> {
    write_json_atomic(&index_path(), &json!({ "version": 1, "pages": pages }), true).await
}

/// 从索引行读取完整页面（含源码）。
async fn full_page_from_row(row: &serde_json::Value) -> PortalResult<NativePageFull> {
    let rel_raw = row.get("relPath").and_then(|v| v.as_str()).unwrap_or("");
    let rel = validate_rel_path(rel_raw)?;
    let source_type = validate_source_type(
        row.get("sourceType")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or(&source_type_from_rel(&rel)),
    )?;
    let source = read_text_opt(&source_abs(&rel))
        .await?
        .ok_or_else(|| PortalError::not_found("native page 源文件不存在"))?;
    Ok(NativePageFull {
        id: row.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        name: row.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        details: row.get("details").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        source_type,
        rel_path: rel_raw.to_string(),
        source,
    })
}

/// 分页列出原生页面（索引项原样返回）。
pub async fn list_native_pages_paged(page: Option<i64>, page_size: Option<i64>) -> PortalResult<serde_json::Value> {
    let p = page.unwrap_or(1).max(1);
    let size = page_size.unwrap_or(20).clamp(1, 200);
    let pages = load_index().await?;
    let total = pages.len() as i64;
    let start = ((p - 1) * size).max(0) as usize;
    let items: Vec<serde_json::Value> = pages.iter().skip(start).take(size as usize).cloned().collect();
    Ok(json!({ "items": items, "total": total, "page": p, "pageSize": size }))
}

/// 按 id 读取单个原生页面（含源码）。
pub async fn get_native_page_by_id(id: &str) -> PortalResult<NativePageFull> {
    let pid = validate_id(id)?;
    let pages = load_index().await?;
    let row = pages
        .iter()
        .find(|r| r.get("id").and_then(|v| v.as_str()) == Some(pid.as_str()))
        .ok_or_else(|| PortalError::not_found("native page 不存在"))?;
    full_page_from_row(row).await
}

/// 批量按 id 取完整原生页面，返回 `{ pages, errors }`。
pub async fn get_native_pages_by_ids(body: &serde_json::Value) -> PortalResult<serde_json::Value> {
    // 支持 { ids: [...] } 或 [...]
    let ids: Vec<String> = if let Some(arr) = body.as_array() {
        arr.iter().filter_map(|v| v.as_str().map(|s| s.trim().to_string())).collect()
    } else if let Some(arr) = body.get("ids").and_then(|v| v.as_array()) {
        arr.iter().filter_map(|v| v.as_str().map(|s| s.trim().to_string())).collect()
    } else {
        return Err(PortalError::bad_request("请求体须为 { ids: string[] } 或 string[]"));
    };
    // 去重 + 去空
    let mut seen = std::collections::HashSet::new();
    let cleaned: Vec<String> = ids.into_iter().filter(|s| !s.is_empty() && seen.insert(s.clone())).collect();

    let pages_index = load_index().await?;
    let mut pages = Vec::new();
    let mut errors = Vec::new();
    for raw_id in cleaned {
        match validate_id(&raw_id) {
            Ok(id) => match pages_index.iter().find(|r| r.get("id").and_then(|v| v.as_str()) == Some(id.as_str())) {
                Some(row) => match full_page_from_row(row).await {
                    Ok(full) => pages.push(serde_json::to_value(full)?),
                    Err(e) => errors.push(json!({ "id": id, "error": e.to_string() })),
                },
                None => errors.push(json!({ "id": id, "error": "native page 不存在" })),
            },
            Err(e) => errors.push(json!({ "id": raw_id, "error": e.to_string() })),
        }
    }
    Ok(json!({ "pages": pages, "errors": errors }))
}

/// 保存原生页面（写源文件 + 索引 upsert）。
pub async fn save_native_page(input: NativePageInput) -> PortalResult<serde_json::Value> {
    let id = validate_id(&input.id)?;
    let st_hint = input
        .source_type
        .clone()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| input.rel_path.as_deref().map(source_type_from_rel))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "js".to_string());
    let source_type = validate_source_type(&st_hint)?;
    let rel_path = validate_rel_path(
        &input
            .rel_path
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| default_rel_path(&id, &source_type)),
    )?;
    if source_type_from_rel(&rel_path) != source_type {
        return Err(PortalError::bad_request("sourceType 与 relPath 扩展名不一致"));
    }
    let source = input.source.ok_or_else(|| PortalError::bad_request("source 必须为字符串"))?;
    let row = json!({
        "id": id,
        "name": input.name.unwrap_or_default(),
        "details": input.details.unwrap_or_default(),
        "sourceType": source_type,
        "relPath": rel_path,
    });

    let _guard = write_lock().lock().await;
    write_text_atomic(&source_abs(&rel_path), &source).await?;
    let mut pages = load_index().await?;
    if let Some(existing) = pages.iter_mut().find(|r| r.get("id").and_then(|v| v.as_str()) == Some(id.as_str())) {
        // 合并（与 Node 的 { ...old, ...row } 一致）
        if let (Some(eo), Some(ro)) = (existing.as_object_mut(), row.as_object()) {
            for (k, v) in ro {
                eo.insert(k.clone(), v.clone());
            }
        }
    } else {
        pages.push(row.clone());
    }
    save_index(&pages).await?;
    Ok(row)
}
