//! HTML 页面存储：分层 v2（分片索引）+ 兼容 v1（扁平 pages-list）。
//!
//! 复刻 Node `lib/htmlPagesStore.js`：
//! - v2：`html-pages/index.json`（域清单）+ `html-pages/index/<domain>.pages.json`（分片）
//!   + `html-pages/sources/<relPath>`（按命名空间分层的 .html）。
//! - v1 兼容：`html-pages/pages-list.json` + `html-pages/sources/<id>.html`（扁平）。
//! - 读：优先分片 → 回退 v1 列表；写：双写（分片 + v1 列表），保证 list 立即可见。
//! - 命名空间：id 点分 `domain.app.module.page`（2-4 段）；无点的旧 id 归 `_legacy` 域。

use serde_json::json;

use crate::config::data_path;
use crate::error::{PortalError, PortalResult};
use crate::fsutil::{read_json_opt, read_text_opt, write_json_atomic, write_text_atomic};
use crate::util::{is_safe_id, is_safe_segment, write_lock};

const LEGACY_DOMAIN: &str = "_legacy";
const MAX_BATCH: usize = 64;

/// 解析出的页面命名空间。
#[derive(Debug, Clone)]
pub struct PageNamespace {
    pub id: String,
    pub domain: String,
    pub app: String,
    pub module: String,
    pub page: String,
    pub rel_path: String,
    pub is_legacy: bool,
}

fn assert_page_id(id: &str) -> PortalResult<String> {
    let t = id.trim();
    if t.is_empty() {
        return Err(PortalError::bad_request("页面 ID 不能为空"));
    }
    if !is_safe_id(t) {
        return Err(PortalError::bad_request(
            "页面 ID 仅允许字母、数字、._-，长度 1–128",
        ));
    }
    Ok(t.to_string())
}

/// 解析页面 id 的命名空间（复刻 `parsePageNamespace`）。
pub fn parse_page_namespace(id: &str) -> PortalResult<PageNamespace> {
    let clean = assert_page_id(id)?;
    let segs: Vec<&str> = clean.split('.').collect();
    for s in &segs {
        if s.is_empty() {
            return Err(PortalError::bad_request(
                "页面 ID 段不能为空（禁止前导/尾随点或连续点）",
            ));
        }
        if !is_safe_segment(s) {
            return Err(PortalError::bad_request(format!(
                "页面 ID 段非法：\"{s}\"（仅允许字母、数字、_-）"
            )));
        }
    }
    if segs.len() == 1 {
        let only = segs[0].to_string();
        return Ok(PageNamespace {
            rel_path: format!("{LEGACY_DOMAIN}/{only}.html"),
            id: clean,
            domain: LEGACY_DOMAIN.to_string(),
            app: String::new(),
            module: String::new(),
            page: only,
            is_legacy: true,
        });
    }
    let domain = segs[0].to_string();
    let page = segs[segs.len() - 1].to_string();
    let middle: Vec<&str> = segs[1..segs.len() - 1].to_vec();
    let app = middle.first().map(|s| s.to_string()).unwrap_or_default();
    let module = middle.get(1).map(|s| s.to_string()).unwrap_or_default();
    let mut rel_parts = vec![domain.clone()];
    rel_parts.extend(middle.iter().map(|s| s.to_string()));
    rel_parts.push(format!("{page}.html"));
    let rel_path = rel_parts.join("/");
    let is_legacy = domain == LEGACY_DOMAIN;
    Ok(PageNamespace {
        id: clean,
        domain,
        app,
        module,
        page,
        rel_path,
        is_legacy,
    })
}

fn list_path() -> std::path::PathBuf {
    data_path(["html-pages", "pages-list.json"])
}
fn top_index_path() -> std::path::PathBuf {
    data_path(["html-pages", "index.json"])
}
fn shard_path(domain: &str) -> std::path::PathBuf {
    data_path(["html-pages", "index", &format!("{domain}.pages.json")])
}
fn source_abs(rel: &str) -> std::path::PathBuf {
    let mut p = data_path(["html-pages", "sources"]);
    for seg in rel.split('/') {
        p.push(seg);
    }
    p
}

/// 读取 v1 列表的 pages 数组。
async fn load_list() -> PortalResult<Vec<serde_json::Value>> {
    Ok(read_json_opt(&list_path())
        .await?
        .and_then(|d| d.get("pages").and_then(|p| p.as_array()).cloned())
        .unwrap_or_default())
}
async fn persist_list(pages: &[serde_json::Value]) -> PortalResult<()> {
    write_json_atomic(&list_path(), &json!({ "version": 1, "pages": pages }), true).await
}

/// 读取分片的 pages 数组。
async fn load_shard(domain: &str) -> PortalResult<Vec<serde_json::Value>> {
    Ok(read_json_opt(&shard_path(domain))
        .await?
        .and_then(|d| d.get("pages").and_then(|p| p.as_array()).cloned())
        .unwrap_or_default())
}
async fn persist_shard(domain: &str, pages: &[serde_json::Value]) -> PortalResult<()> {
    write_json_atomic(
        &shard_path(domain),
        &json!({ "version": 1, "domain": domain, "pages": pages }),
        true,
    )
    .await
}

async fn load_top_domains() -> PortalResult<Vec<String>> {
    Ok(read_json_opt(&top_index_path())
        .await?
        .and_then(|d| d.get("domains").and_then(|x| x.as_array()).cloned())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default())
}
async fn persist_top_domains(domains: &[String]) -> PortalResult<()> {
    let mut set: Vec<String> = domains.to_vec();
    set.sort();
    set.dedup();
    write_json_atomic(
        &top_index_path(),
        &json!({ "version": 2, "domains": set }),
        true,
    )
    .await
}

/// 防穿越：relPath 不得绝对 / 含 `..` / 反斜杠。
fn safe_rel(rel: &str) -> PortalResult<String> {
    if rel.starts_with('/') || rel.contains("..") || rel.contains('\\') {
        return Err(PortalError::bad_request(
            "relPath 非法（含 ..、\\ 或前导 /）",
        ));
    }
    Ok(rel.to_string())
}

/// 由 row 解析出 html 源码绝对路径（v2 relPath 优先，v1 latestHtmlFile 次之）。
fn resolve_html_abs(row: &serde_json::Value) -> PortalResult<std::path::PathBuf> {
    if let Some(rel) = row
        .get("relPath")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    {
        let safe = safe_rel(rel)?;
        return Ok(source_abs(&safe));
    }
    if let Some(latest) = row
        .get("latestHtmlFile")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    {
        let base = std::path::Path::new(latest)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        if !base.to_lowercase().ends_with(".html") {
            return Err(PortalError::bad_request("页面源码须为 .html 文件"));
        }
        return Ok(source_abs(&base));
    }
    let id = row.get("id").and_then(|v| v.as_str()).unwrap_or("").trim();
    if id.is_empty() {
        return Err(PortalError::business(
            "页面记录缺少 id / relPath / latestHtmlFile",
        ));
    }
    Ok(source_abs(&format!("{id}.html")))
}

/// 跨分片/列表查 row。
async fn find_row_anywhere(id: &str) -> PortalResult<Option<serde_json::Value>> {
    if let Ok(ns) = parse_page_namespace(id) {
        let shard = load_shard(&ns.domain).await?;
        if let Some(r) = shard
            .into_iter()
            .find(|r| r.get("id").and_then(|v| v.as_str()) == Some(id))
        {
            return Ok(Some(r));
        }
    }
    let list = load_list().await?;
    Ok(list
        .into_iter()
        .find(|r| r.get("id").and_then(|v| v.as_str()) == Some(id)))
}

/// 由 row 读取完整页面（含 html，带 v2→v1 回退）。
async fn read_full_from_row(row: &serde_json::Value) -> PortalResult<serde_json::Value> {
    let abs = resolve_html_abs(row)?;
    let mut html = read_text_opt(&abs).await?;
    if html.is_none() {
        // v2 relPath 失败时回退到 v1 latestHtmlFile 扁平文件
        if row.get("relPath").is_some()
            && let Some(latest) = row
                .get("latestHtmlFile")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
        {
            let base = std::path::Path::new(latest)
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            html = read_text_opt(&source_abs(&base)).await?;
        }
    }
    let html = html.ok_or_else(|| PortalError::not_found("HTML 源码文件缺失或损坏"))?;
    let id = row.get("id").and_then(|v| v.as_str()).unwrap_or("");
    Ok(json!({
        "id": id,
        "name": row.get("name").and_then(|v| v.as_str()).unwrap_or(""),
        "details": row.get("details").and_then(|v| v.as_str()).unwrap_or(""),
        "domain": row.get("domain"),
        "app": row.get("app"),
        "module": row.get("module"),
        "relPath": row.get("relPath"),
        "latestHtmlFile": row.get("latestHtmlFile").and_then(|v| v.as_str()).map(|s| s.to_string())
            .unwrap_or_else(|| format!("{id}.html")),
        "html": html,
    }))
}

/// 分页列表（带 domain/app/module 过滤）。
pub async fn list_html_pages_paged(
    page: Option<i64>,
    page_size: Option<i64>,
    f_domain: Option<&str>,
    f_app: Option<&str>,
    f_module: Option<&str>,
) -> PortalResult<serde_json::Value> {
    let p = page.unwrap_or(1).max(1);
    let size = page_size.unwrap_or(20).clamp(1, 200);
    let pages = load_list().await?;
    let fd = f_domain.unwrap_or("").trim();
    let fa = f_app.unwrap_or("").trim();
    let fm = f_module.unwrap_or("").trim();
    let filtered: Vec<&serde_json::Value> = pages
        .iter()
        .filter(|r| {
            if !fd.is_empty() {
                let rd = r
                    .get("domain")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .unwrap_or("_legacy");
                if rd != fd {
                    return false;
                }
            }
            if !fa.is_empty() && r.get("app").and_then(|v| v.as_str()).unwrap_or("") != fa {
                return false;
            }
            if !fm.is_empty() && r.get("module").and_then(|v| v.as_str()).unwrap_or("") != fm {
                return false;
            }
            true
        })
        .collect();
    let total = filtered.len() as i64;
    let start = ((p - 1) * size).max(0) as usize;
    let items: Vec<serde_json::Value> = filtered
        .into_iter()
        .skip(start)
        .take(size as usize)
        .map(|r| {
            let id = r.get("id").and_then(|v| v.as_str()).unwrap_or("");
            let latest = r
                .get("latestHtmlFile")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("{id}.html"));
            json!({
                "id": id,
                "name": r.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                "details": r.get("details").and_then(|v| v.as_str()).unwrap_or(""),
                "latestHtmlFile": latest,
                "relPath": r.get("relPath"),
                "domain": r.get("domain"),
                "app": r.get("app"),
                "module": r.get("module"),
            })
        })
        .collect();
    Ok(json!({ "items": items, "total": total, "page": p, "pageSize": size }))
}

/// 保存入参。
#[derive(Debug, Clone, serde::Deserialize)]
pub struct HtmlPageInput {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub details: Option<String>,
    #[serde(default)]
    pub html: Option<String>,
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub app: Option<String>,
    #[serde(default)]
    pub module: Option<String>,
}

/// 保存页面（写源文件 + v2 分片 + v1 列表双写）。
pub async fn save_html_page(input: HtmlPageInput) -> PortalResult<serde_json::Value> {
    let id = assert_page_id(&input.id)?;
    let name = input.name.unwrap_or_default();
    let details = input.details.unwrap_or_default();
    let html = input
        .html
        .ok_or_else(|| PortalError::bad_request("html 必须为字符串"))?;
    let ns = parse_page_namespace(&id)?;
    let domain = input
        .domain
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| ns.domain.clone());
    let app = input
        .app
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| ns.app.clone());
    let module = input
        .module
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| ns.module.clone());
    let rel_path = ns.rel_path.clone();
    let latest = if ns.is_legacy {
        format!("{}.html", ns.page)
    } else {
        // posix basename of relPath
        rel_path.rsplit('/').next().unwrap_or(&rel_path).to_string()
    };

    let _guard = write_lock().lock().await;
    write_text_atomic(&source_abs(&rel_path), &html).await?;

    let row = json!({
        "id": id, "name": name, "details": details,
        "domain": domain, "app": app, "module": module, "page": ns.page,
        "relPath": rel_path, "latestHtmlFile": latest,
    });

    // v2 分片 upsert
    let mut shard = load_shard(&domain).await?;
    if let Some(existing) = shard
        .iter_mut()
        .find(|r| r.get("id").and_then(|v| v.as_str()) == Some(id.as_str()))
    {
        *existing = row.clone();
    } else {
        shard.push(row.clone());
    }
    persist_shard(&domain, &shard).await?;
    let mut domains = load_top_domains().await?;
    if !domains.iter().any(|d| d == &domain) {
        domains.push(domain.clone());
        persist_top_domains(&domains).await?;
    }

    // v1 列表 upsert（合并）
    let mut list = load_list().await?;
    if let Some(existing) = list
        .iter_mut()
        .find(|r| r.get("id").and_then(|v| v.as_str()) == Some(id.as_str()))
    {
        if let (Some(eo), Some(ro)) = (existing.as_object_mut(), row.as_object()) {
            for (k, v) in ro {
                eo.insert(k.clone(), v.clone());
            }
        }
    } else {
        list.push(row.clone());
    }
    persist_list(&list).await?;

    Ok(row)
}

/// 按 id 读取完整页面。
pub async fn get_html_page_by_id(id: &str) -> PortalResult<serde_json::Value> {
    let pid = assert_page_id(id)?;
    let row = find_row_anywhere(&pid)
        .await?
        .ok_or_else(|| PortalError::not_found("页面不存在"))?;
    read_full_from_row(&row).await
}

/// 批量按 id 取完整页面（domain 分桶 + 分片缓存），返回 `{ pages, errors }`。
pub async fn get_html_pages_by_ids(body: &serde_json::Value) -> PortalResult<serde_json::Value> {
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
            "请求体须为 { ids: string[] } 或 JSON 字符串数组",
        ));
    };
    let mut seen = std::collections::HashSet::new();
    let cleaned: Vec<String> = ids
        .into_iter()
        .filter(|s| !s.is_empty() && seen.insert(s.clone()))
        .collect();
    if cleaned.is_empty() {
        return Err(PortalError::bad_request("ids 不能为空"));
    }
    if cleaned.len() > MAX_BATCH {
        return Err(PortalError::bad_request(format!(
            "单次最多 {MAX_BATCH} 个页面 ID"
        )));
    }

    // domain 分桶
    let mut by_domain: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    let mut errors: Vec<serde_json::Value> = Vec::new();
    for raw in &cleaned {
        match parse_page_namespace(raw) {
            Ok(ns) => by_domain.entry(ns.domain).or_default().push(raw.clone()),
            Err(e) => errors.push(json!({ "id": raw, "error": e.to_string() })),
        }
    }

    let mut shard_cache: std::collections::HashMap<String, Vec<serde_json::Value>> =
        std::collections::HashMap::new();
    let mut legacy_list: Option<Vec<serde_json::Value>> = None;
    let mut pages: Vec<serde_json::Value> = Vec::new();

    for (domain, id_list) in by_domain {
        if !shard_cache.contains_key(&domain) {
            shard_cache.insert(domain.clone(), load_shard(&domain).await?);
        }
        let shard = shard_cache.get(&domain).cloned().unwrap_or_default();
        for id in id_list {
            let mut row = shard
                .iter()
                .find(|r| r.get("id").and_then(|v| v.as_str()) == Some(id.as_str()))
                .cloned();
            if row.is_none() {
                if legacy_list.is_none() {
                    legacy_list = Some(load_list().await?);
                }
                row = legacy_list.as_ref().and_then(|l| {
                    l.iter()
                        .find(|r| r.get("id").and_then(|v| v.as_str()) == Some(id.as_str()))
                        .cloned()
                });
            }
            match row {
                Some(r) => match read_full_from_row(&r).await {
                    Ok(full) => pages.push(full),
                    Err(e) => errors.push(json!({ "id": id, "error": e.to_string() })),
                },
                None => errors.push(json!({ "id": id, "error": "页面不存在" })),
            }
        }
    }

    Ok(json!({ "pages": pages, "errors": errors }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn namespace_parsing() {
        let ns = parse_page_namespace("fi.payroll.salary.detail").unwrap();
        assert_eq!(ns.domain, "fi");
        assert_eq!(ns.app, "payroll");
        assert_eq!(ns.module, "salary");
        assert_eq!(ns.page, "detail");
        assert_eq!(ns.rel_path, "fi/payroll/salary/detail.html");
        assert!(!ns.is_legacy);

        let legacy = parse_page_namespace("ctn_view1").unwrap();
        assert_eq!(legacy.domain, "_legacy");
        assert!(legacy.is_legacy);
        assert_eq!(legacy.rel_path, "_legacy/ctn_view1.html");

        let two = parse_page_namespace("fi.welcome").unwrap();
        assert_eq!(two.domain, "fi");
        assert_eq!(two.app, "");
        assert_eq!(two.page, "welcome");
        assert_eq!(two.rel_path, "fi/welcome.html");

        // 空段拒绝
        assert!(parse_page_namespace("a..b").is_err());
        assert!(parse_page_namespace(".a").is_err());
    }
}
