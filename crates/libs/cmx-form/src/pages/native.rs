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
    /// 原生页面唯一标识（点分命名空间，仅允许字母数字._-）。
    #[serde(default)]
    pub id: String,
    /// 页面名称。
    #[serde(default)]
    pub name: Option<String>,
    /// 页面描述。
    #[serde(default)]
    pub details: Option<String>,
    /// 源码类型（js/html）。
    #[serde(default, rename = "sourceType")]
    pub source_type: Option<String>,
    /// 源码文本（必填）。
    #[serde(default)]
    pub source: Option<String>,
    /// 源文件相对路径（缺省由 id + sourceType 推导）。
    #[serde(default, rename = "relPath")]
    pub rel_path: Option<String>,
}

/// 完整页面（含源码）。
#[derive(Debug, Clone, Serialize)]
pub struct NativePageFull {
    /// 原生页面唯一标识。
    pub id: String,
    /// 页面名称。
    pub name: String,
    /// 页面描述。
    pub details: String,
    /// 源码类型（js/html）。
    #[serde(rename = "sourceType")]
    pub source_type: String,
    /// 源文件相对路径。
    #[serde(rename = "relPath")]
    pub rel_path: String,
    /// 源码文本。
    pub source: String,
}

/// 索引文件路径（`native-pages/index.json`）。
fn index_path() -> std::path::PathBuf {
    data_path(["native-pages", "index.json"])
}

/// 由相对路径拼源文件绝对路径（`native-pages/sources/<rel>`）。
fn source_abs(rel: &str) -> std::path::PathBuf {
    let mut p = data_path(["native-pages", "sources"]);
    for seg in rel.split('/') {
        p.push(seg);
    }
    p
}

/// 校验 native page id（整体 safe-id + 每段须为 safe-segment）。
fn validate_id(id: &str) -> PortalResult<String> {
    let s = id.trim();
    if s.is_empty() || !is_safe_id(s) {
        return Err(PortalError::bad_request(
            "native page id 仅允许字母、数字、._-，长度 1-128",
        ));
    }
    // 点分段：每段须为安全段，禁止空段
    for part in s.split('.') {
        if part.is_empty() || !is_safe_segment(part) {
            return Err(PortalError::bad_request("native page id 段非法"));
        }
    }
    Ok(s.to_string())
}

/// 由 relPath 扩展名推断 sourceType（.js/.mjs → js，.html/.htm → html，其余空串）。
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

/// 校验 sourceType 仅支持 js/html。
fn validate_source_type(st: &str) -> PortalResult<String> {
    let t = st.trim().to_lowercase();
    if t == "js" || t == "html" {
        Ok(t)
    } else {
        Err(PortalError::bad_request("sourceType 仅支持 js/html"))
    }
}

/// 由 id + sourceType 推导默认 relPath（id 点分转斜杠 + 扩展名）。
fn default_rel_path(id: &str, source_type: &str) -> String {
    // id 点分转斜杠路径
    let base = id.split('.').collect::<Vec<_>>().join("/");
    let ext = if source_type == "html" { "html" } else { "js" };
    format!("{base}.{ext}")
}

/// 校验并规范化 relPath（posix），防穿越。
///
/// 拒绝绝对路径、反斜杠、`..` 段，且扩展名须为 js/mjs/html/htm。
fn validate_rel_path(rel: &str) -> PortalResult<String> {
    let raw = rel.trim();
    // 拒绝空、绝对路径、反斜杠
    if raw.is_empty() || raw.starts_with('/') || raw.contains('\\') {
        return Err(PortalError::bad_request("relPath 非法"));
    }
    // 简易 posix 规范：拒绝任何 `..` 段
    if raw.split('/').any(|seg| seg == "..") {
        return Err(PortalError::bad_request("relPath 非法"));
    }
    // 扩展名须受支持
    if source_type_from_rel(raw).is_empty() {
        return Err(PortalError::bad_request("源文件仅支持 .js/.mjs/.html/.htm"));
    }
    Ok(raw.to_string())
}

/// 读取索引的 pages 数组（缺失返回空）。
async fn load_index() -> PortalResult<Vec<serde_json::Value>> {
    match read_json_opt(&index_path()).await? {
        Some(doc) => Ok(doc
            .get("pages")
            .and_then(|p| p.as_array())
            .cloned()
            .unwrap_or_default()),
        None => Ok(Vec::new()),
    }
}

/// 持久化索引（覆盖写，pretty 格式）。
async fn save_index(pages: &[serde_json::Value]) -> PortalResult<()> {
    write_json_atomic(
        &index_path(),
        &json!({ "version": 1, "pages": pages }),
        true,
    )
    .await
}

/// 从索引行读取完整页面（含源码）。
///
/// 校验 relPath/sourceType 后读取源文件，组装 [`NativePageFull`]。
async fn full_page_from_row(row: &serde_json::Value) -> PortalResult<NativePageFull> {
    let rel_raw = row.get("relPath").and_then(|v| v.as_str()).unwrap_or("");
    let rel = validate_rel_path(rel_raw)?;
    // sourceType 优先取声明值，否则由 relPath 推断
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
        id: row
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        name: row
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        details: row
            .get("details")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        source_type,
        rel_path: rel_raw.to_string(),
        source,
    })
}

/// 分页列出原生页面（索引项原样返回）。
///
/// # Arguments
///
/// * `page` - 页码（从 1 起，缺省 1）。
/// * `page_size` - 每页条数（缺省 20，范围 1–200）。
///
/// # Returns
///
/// 返回 `{ items, total, page, pageSize }`，items 为索引项原样 JSON。
pub async fn list_native_pages_paged(
    page: Option<i64>,
    page_size: Option<i64>,
) -> PortalResult<serde_json::Value> {
    // 归一页码与每页条数
    let p = page.unwrap_or(1).max(1);
    let size = page_size.unwrap_or(20).clamp(1, 200);
    let pages = load_index().await?;
    let total = pages.len() as i64;
    let start = ((p - 1) * size).max(0) as usize;
    // 切片当前页
    let items: Vec<serde_json::Value> = pages
        .iter()
        .skip(start)
        .take(size as usize)
        .cloned()
        .collect();
    Ok(json!({ "items": items, "total": total, "page": p, "pageSize": size }))
}

/// 按 id 读取单个原生页面（含源码）。
///
/// # Arguments
///
/// * `id` - 原生页面唯一标识。
///
/// # Returns
///
/// 返回 [`NativePageFull`]；页面不存在返回 `PortalError::NotFound`。
pub async fn get_native_page_by_id(id: &str) -> PortalResult<NativePageFull> {
    let pid = validate_id(id)?;
    let pages = load_index().await?;
    // 在索引中定位记录
    let row = pages
        .iter()
        .find(|r| r.get("id").and_then(|v| v.as_str()) == Some(pid.as_str()))
        .ok_or_else(|| PortalError::not_found("native page 不存在"))?;
    full_page_from_row(row).await
}

/// 批量按 id 取完整原生页面，返回 `{ pages, errors }`。
///
/// # Arguments
///
/// * `body` - 请求体，支持 `{ ids: string[] }` 或顶层数组。
///
/// # Returns
///
/// 返回 `{ pages, errors }`，单条失败不阻断，记入 errors。
pub async fn get_native_pages_by_ids(body: &serde_json::Value) -> PortalResult<serde_json::Value> {
    // 支持 { ids: [...] } 或 [...]
    let ids: Vec<String> = if let Some(arr) = body.as_array() {
        arr.iter()
            .filter_map(|v| v.as_str().map(|s| s.trim().to_string()))
            .collect()
    } else if let Some(arr) = body.get("ids").and_then(|v| v.as_array()) {
        arr.iter()
            .filter_map(|v| v.as_str().map(|s| s.trim().to_string()))
            .collect()
    } else {
        return Err(PortalError::bad_request(
            "请求体须为 { ids: string[] } 或 string[]",
        ));
    };
    // 去重 + 去空
    let mut seen = std::collections::HashSet::new();
    let cleaned: Vec<String> = ids
        .into_iter()
        .filter(|s| !s.is_empty() && seen.insert(s.clone()))
        .collect();

    let pages_index = load_index().await?;
    let mut pages = Vec::new();
    let mut errors = Vec::new();
    // 逐个解析：id 非法/不存在/源码缺失分别记入 errors
    for raw_id in cleaned {
        match validate_id(&raw_id) {
            Ok(id) => match pages_index
                .iter()
                .find(|r| r.get("id").and_then(|v| v.as_str()) == Some(id.as_str()))
            {
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
///
/// # Arguments
///
/// * `input` - 保存入参（id/name/details/sourceType/source/relPath）。
///
/// # Returns
///
/// 返回新写入的索引行 JSON。
pub async fn save_native_page(input: NativePageInput) -> PortalResult<serde_json::Value> {
    let id = validate_id(&input.id)?;
    // sourceType 优先级：显式声明 > relPath 推断 > 默认 js
    let st_hint = input
        .source_type
        .clone()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| input.rel_path.as_deref().map(source_type_from_rel))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "js".to_string());
    let source_type = validate_source_type(&st_hint)?;
    // relPath 优先级：显式声明 > id + sourceType 推导
    let rel_path = validate_rel_path(
        &input
            .rel_path
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| default_rel_path(&id, &source_type)),
    )?;
    // sourceType 与 relPath 扩展名须一致
    if source_type_from_rel(&rel_path) != source_type {
        return Err(PortalError::bad_request(
            "sourceType 与 relPath 扩展名不一致",
        ));
    }
    let source = input
        .source
        .ok_or_else(|| PortalError::bad_request("source 必须为字符串"))?;
    let row = json!({
        "id": id,
        "name": input.name.unwrap_or_default(),
        "details": input.details.unwrap_or_default(),
        "sourceType": source_type,
        "relPath": rel_path,
    });

    // 全局写锁串行化
    let _guard = write_lock().lock().await;
    // 写源文件
    write_text_atomic(&source_abs(&rel_path), &source).await?;
    // 索引 upsert：存在则合并，否则追加
    let mut pages = load_index().await?;
    if let Some(existing) = pages
        .iter_mut()
        .find(|r| r.get("id").and_then(|v| v.as_str()) == Some(id.as_str()))
    {
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
