//! HTML 页面存储：分层 v2（分片索引）+ 兼容 v1（扁平 pages-list）。
//!
//! 复刻 Node `lib/htmlPagesStore.js`：
//! - v2：`html-pages/index.json`（域清单）+ `html-pages/index/<domain>.pages.json`（分片）
//!   + `html-pages/sources/<relPath>`（按命名空间分层的 .html）。
//! - v1 兼容：`html-pages/pages-list.json` + `html-pages/sources/<id>.html`（扁平）。
//! - 读：优先分片 → 回退 v1 列表；写：双写（分片 + v1 列表），保证 list 立即可见。
//! - 命名空间：id 点分 `domain.app.module.page`（2-4 段）；无点的旧 id 归 `_legacy` 域。

use serde_json::json;

use crate::cache::{cached_read_json, cached_read_text, content_rev_with_meta, invalidate_paths};
use crate::config::data_path;
use crate::error::{PortalError, PortalResult};
use crate::fsutil::{write_json_atomic, write_text_atomic};
use crate::util::{is_safe_id, is_safe_segment, write_lock};

/// 旧式无命名空间页面归入的虚拟域。
const LEGACY_DOMAIN: &str = "_legacy";
/// 批量读取单次最大页面数。
const MAX_BATCH: usize = 64;

/// 解析出的页面命名空间。
#[derive(Debug, Clone)]
pub struct PageNamespace {
    /// 原始页面 id。
    pub id: String,
    /// 域（id 首段，旧式 id 归 `_legacy`）。
    pub domain: String,
    /// 应用（id 中间段首项，可能为空）。
    pub app: String,
    /// 模块（id 中间段次项，可能为空）。
    pub module: String,
    /// 页面名（id 末段）。
    pub page: String,
    /// 源文件相对路径（由命名空间推导）。
    pub rel_path: String,
    /// 是否为旧式无命名空间页面。
    pub is_legacy: bool,
}

/// 断言页面 id 非空且为 safe-id。
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
///
/// 单段 id 归 `_legacy` 域；多段 id（2–4 段 `domain[.app[.module]].page`）拆分各段。
/// 每段须为 safe-segment，禁止空段。
///
/// # Arguments
///
/// * `id` - 页面 id（点分命名空间或旧式单段）。
///
/// # Returns
///
/// 返回解析出的 [`PageNamespace`]；id 非法（空、段非法）返回 `PortalError::BadRequest`。
pub fn parse_page_namespace(id: &str) -> PortalResult<PageNamespace> {
    let clean = assert_page_id(id)?;
    let segs: Vec<&str> = clean.split('.').collect();
    // 逐段校验：禁止空段与非法段
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
    // 单段：归 _legacy 域
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
    // 多段：首段为 domain，末段为 page，中间为 app/module
    let domain = segs[0].to_string();
    let page = segs[segs.len() - 1].to_string();
    let middle: Vec<&str> = segs[1..segs.len() - 1].to_vec();
    let app = middle.first().map(|s| s.to_string()).unwrap_or_default();
    let module = middle.get(1).map(|s| s.to_string()).unwrap_or_default();
    // 拼源文件相对路径：domain/middle.../page.html
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

/// v1 列表文件路径（`html-pages/pages-list.json`）。
fn list_path() -> std::path::PathBuf {
    data_path(["html-pages", "pages-list.json"])
}
/// v2 域清单文件路径（`html-pages/index.json`）。
fn top_index_path() -> std::path::PathBuf {
    data_path(["html-pages", "index.json"])
}
/// v2 分片文件路径（`html-pages/index/<domain>.pages.json`）。
fn shard_path(domain: &str) -> std::path::PathBuf {
    data_path(["html-pages", "index", &format!("{domain}.pages.json")])
}
/// 由相对路径拼源文件绝对路径（`html-pages/sources/<rel>`）。
fn source_abs(rel: &str) -> std::path::PathBuf {
    let mut p = data_path(["html-pages", "sources"]);
    for seg in rel.split('/') {
        p.push(seg);
    }
    p
}

/// 读取 v1 列表的 pages 数组。
async fn load_list() -> PortalResult<Vec<serde_json::Value>> {
    Ok(cached_read_json(&list_path())
        .await?
        .and_then(|d| d.get("pages").and_then(|p| p.as_array()).cloned())
        .unwrap_or_default())
}
/// 持久化 v1 列表（覆盖写）。
async fn persist_list(pages: &[serde_json::Value]) -> PortalResult<()> {
    write_json_atomic(&list_path(), &json!({ "version": 1, "pages": pages }), true).await
}

/// 读取分片的 pages 数组。
async fn load_shard(domain: &str) -> PortalResult<Vec<serde_json::Value>> {
    Ok(cached_read_json(&shard_path(domain))
        .await?
        .and_then(|d| d.get("pages").and_then(|p| p.as_array()).cloned())
        .unwrap_or_default())
}
/// 持久化某域分片（覆盖写，带 domain 标记）。
async fn persist_shard(domain: &str, pages: &[serde_json::Value]) -> PortalResult<()> {
    write_json_atomic(
        &shard_path(domain),
        &json!({ "version": 1, "domain": domain, "pages": pages }),
        true,
    )
    .await
}

/// 读取顶层域清单。
async fn load_top_domains() -> PortalResult<Vec<String>> {
    Ok(cached_read_json(&top_index_path())
        .await?
        .and_then(|d| d.get("domains").and_then(|x| x.as_array()).cloned())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default())
}
/// 持久化顶层域清单（排序去重后写）。
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
///
/// 三级回退：relPath → latestHtmlFile（取 basename）→ `<id>.html`。
fn resolve_html_abs(row: &serde_json::Value) -> PortalResult<std::path::PathBuf> {
    // 优先 v2 relPath
    if let Some(rel) = row
        .get("relPath")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    {
        let safe = safe_rel(rel)?;
        return Ok(source_abs(&safe));
    }
    // 次选 v1 latestHtmlFile（取 basename 防穿越）
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
    // 兜底：<id>.html
    let id = row.get("id").and_then(|v| v.as_str()).unwrap_or("").trim();
    if id.is_empty() {
        return Err(PortalError::business(
            "页面记录缺少 id / relPath / latestHtmlFile",
        ));
    }
    Ok(source_abs(&format!("{id}.html")))
}

/// 跨分片/列表查 row。
///
/// 先按命名空间域查 v2 分片，未命中再查 v1 列表。
async fn find_row_anywhere(id: &str) -> PortalResult<Option<serde_json::Value>> {
    // 先查 v2 分片
    if let Ok(ns) = parse_page_namespace(id) {
        let shard = load_shard(&ns.domain).await?;
        if let Some(r) = shard
            .into_iter()
            .find(|r| r.get("id").and_then(|v| v.as_str()) == Some(id))
        {
            return Ok(Some(r));
        }
    }
    // 回退 v1 列表
    let list = load_list().await?;
    Ok(list
        .into_iter()
        .find(|r| r.get("id").and_then(|v| v.as_str()) == Some(id)))
}

/// 由 row 读取完整页面（含 html，带 v2→v1 回退）。
///
/// v2 relPath 源文件缺失时回退到 v1 latestHtmlFile 扁平文件。
/// `rev` 实时由已读 html 算出（xxhash64 → 16 hex），作 ETag / 前端缓存校验锚点。
/// 方案2：不依赖索引行存储的 rev，读时现算，天然与源文件一致。
async fn read_full_from_row(row: &serde_json::Value) -> PortalResult<serde_json::Value> {
    let abs = resolve_html_abs(row)?;
    let mut html = cached_read_text(&abs).await?;
    // v2 relPath 失败时回退到 v1 latestHtmlFile
    if html.is_none()
        && row.get("relPath").is_some()
        && let Some(latest) = row
            .get("latestHtmlFile")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
    {
        let base = std::path::Path::new(latest)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        html = cached_read_text(&source_abs(&base)).await?;
    }
    let html = html.ok_or_else(|| PortalError::not_found("HTML 源码文件缺失或损坏"))?;
    let id = row.get("id").and_then(|v| v.as_str()).unwrap_or("");
    // rev 实时算：源码内容 + 行字段 canonical（domain|app|module|doc|name|details|relPath，
    // null/缺失归一空串）。行字段参与哈希后，服务端只改坐标（不动 html）也能让前端
    // IndexedDB 缓存失效重拉，坐标随缓存自愈传播。
    let field = |k: &str| row.get(k).and_then(|v| v.as_str()).unwrap_or("");
    let rev = content_rev_with_meta(
        &[
            field("domain"),
            field("app"),
            field("module"),
            field("doc"),
            field("name"),
            field("details"),
            field("relPath"),
        ],
        &html,
    );
    Ok(json!({
        "id": id,
        "name": row.get("name").and_then(|v| v.as_str()).unwrap_or(""),
        "details": row.get("details").and_then(|v| v.as_str()).unwrap_or(""),
        "domain": row.get("domain"),
        "app": row.get("app"),
        "module": row.get("module"),
        "doc": row.get("doc"),
        "relPath": row.get("relPath"),
        "rev": rev,
        "latestHtmlFile": row.get("latestHtmlFile").and_then(|v| v.as_str()).map(|s| s.to_string())
            .unwrap_or_else(|| format!("{id}.html")),
        "html": html,
    }))
}

/// 分页列表（带 keyword 搜索 + domain/app/module 过滤）。
///
/// # Arguments
///
/// * `page` - 页码（从 1 起，缺省 1）。
/// * `page_size` - 每页条数（缺省 20，范围 1–200）。
/// * `f_domain` / `f_app` / `f_module` - 可选过滤条件，`None` 表示不过滤。
/// * `f_keyword` - 可选关键词，对 id/name/details 做不区分大小写的包含匹配。
///
/// # Returns
///
/// 返回 `{ items, total, page, pageSize }`，items 为列表摘要 JSON。
pub async fn list_html_pages_paged(
    page: Option<i64>,
    page_size: Option<i64>,
    f_domain: Option<&str>,
    f_app: Option<&str>,
    f_module: Option<&str>,
    f_keyword: Option<&str>,
) -> PortalResult<serde_json::Value> {
    // 归一页码、每页条数与过滤参数
    let p = page.unwrap_or(1).max(1);
    let size = page_size.unwrap_or(20).clamp(1, 200);
    let pages = load_list().await?;
    let fd = f_domain.unwrap_or("").trim();
    let fa = f_app.unwrap_or("").trim();
    let fm = f_module.unwrap_or("").trim();
    // keyword：trim 后转小写，对 id/name/details 任一做包含匹配
    let fk = f_keyword
        .unwrap_or("")
        .trim()
        .to_lowercase();
    // 逐条应用 keyword + domain/app/module 过滤（domain 缺省归 _legacy）
    let filtered: Vec<&serde_json::Value> = pages
        .iter()
        .filter(|r| {
            if !fk.is_empty() {
                let id = r.get("id").and_then(|v| v.as_str()).unwrap_or("");
                let name = r.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let details = r.get("details").and_then(|v| v.as_str()).unwrap_or("");
                let hay = format!("{id}\n{name}\n{details}").to_lowercase();
                if !hay.contains(&fk) {
                    return false;
                }
            }
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
    // 切片当前页并映射为摘要
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
                "doc": r.get("doc"),
            })
        })
        .collect();
    Ok(json!({ "items": items, "total": total, "page": p, "pageSize": size }))
}

/// 保存入参。
#[derive(Debug, Clone, serde::Deserialize)]
pub struct HtmlPageInput {
    /// 页面唯一标识。
    #[serde(default)]
    pub id: String,
    /// 页面名称。
    #[serde(default)]
    pub name: Option<String>,
    /// 页面描述。
    #[serde(default)]
    pub details: Option<String>,
    /// HTML 源码（必填）。
    #[serde(default)]
    pub html: Option<String>,
    /// 域（缺省由 id 命名空间推导）。
    #[serde(default)]
    pub domain: Option<String>,
    /// 应用（缺省由 id 命名空间推导）。
    #[serde(default)]
    pub app: Option<String>,
    /// 模块（缺省由 id 命名空间推导）。
    #[serde(default)]
    pub module: Option<String>,
    /// 单据模块编码 moduleCode（绑定页面加载的业务单据；缺省无）。
    #[serde(default)]
    pub doc: Option<String>,
}

/// 保存页面（写源文件 + v2 分片 + v1 列表双写）。
///
/// # Arguments
///
/// * `input` - 保存入参。
///
/// # Returns
///
/// 返回新写入的行 JSON。
pub async fn save_html_page(input: HtmlPageInput) -> PortalResult<serde_json::Value> {
    let id = assert_page_id(&input.id)?;
    let name = input.name.unwrap_or_default();
    let details = input.details.unwrap_or_default();
    let html = input
        .html
        .ok_or_else(|| PortalError::bad_request("html 必须为字符串"))?;
    // 解析命名空间，推导 domain/app/module/relPath
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
    let doc = input
        .doc
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let rel_path = ns.rel_path.clone();
    // latestHtmlFile：旧式用 <page>.html，否则取 relPath basename
    let latest = if ns.is_legacy {
        format!("{}.html", ns.page)
    } else {
        // posix basename of relPath
        rel_path.rsplit('/').next().unwrap_or(&rel_path).to_string()
    };

    // 全局写锁串行化
    let _guard = write_lock().lock().await;
    // 写源文件
    let src_path = source_abs(&rel_path);
    write_text_atomic(&src_path, &html).await?;

    // 注：rev 不再写入索引行（方案2：读路径实时算 hash，索引保持纯净）。
    let row = json!({
        "id": id, "name": name, "details": details,
        "domain": domain, "app": app, "module": module, "page": ns.page,
        "doc": doc, "relPath": rel_path, "latestHtmlFile": latest,
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
    // 维护顶层域清单
    let mut domains = load_top_domains().await?;
    let mut top_dirty = false;
    if !domains.iter().any(|d| d == &domain) {
        domains.push(domain.clone());
        top_dirty = true;
    }
    if top_dirty {
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

    // 失效本进程 L1 缓存：源文件（内容变了）+ 变更的索引文件。
    // rev 实时算，不存索引，故跨节点靠源文件内容天然一致；moka 各自 TTL 收敛。
    let list_p = list_path();
    let shard_p = shard_path(&domain);
    let top_p = top_index_path();
    let mut to_invalidate: Vec<&std::path::Path> = vec![src_path.as_path(), list_p.as_path(), shard_p.as_path()];
    if top_dirty {
        to_invalidate.push(top_p.as_path());
    }
    invalidate_paths(&to_invalidate).await;

    Ok(row)
}

/// 按 id 读取完整页面。
///
/// # Arguments
///
/// * `id` - 页面唯一标识。
///
/// # Returns
///
/// 返回含 html 源码的完整页面 JSON；页面不存在返回 `PortalError::NotFound`。
pub async fn get_html_page_by_id(id: &str) -> PortalResult<serde_json::Value> {
    let pid = assert_page_id(id)?;
    // 跨分片/列表查找
    let row = find_row_anywhere(&pid)
        .await?
        .ok_or_else(|| PortalError::not_found("页面不存在"))?;
    read_full_from_row(&row).await
}

/// 批量按 id 取页面，支持 `clientRevs` 差异同步，返回 `{ pages, revs, errors }`。
///
/// # 差异同步协议（详见方案 2.3）
///
/// 请求体可选 `clientRevs: { id → rev }`：
/// - 缺省 / 空 → 全量返回所有 page 的 body（向后兼容老前端）。
/// - 存在 → 仅当 `clientRevs[id] !== 索引行 rev` 时才读源文件返回 body；命中（相等）则省略 body。
///
/// `revs` 始终返回全量 `{ id → rev }` 清单，供前端刷新本地 rev 表。
///
/// # Arguments
///
/// * `body` - 请求体，支持 `{ ids, clientRevs? }` 或顶层 ids 数组。
///
/// # Returns
///
/// 返回 `{ pages, revs, errors }`；单条失败不阻断，记入 errors。
pub async fn get_html_pages_by_ids(body: &serde_json::Value) -> PortalResult<serde_json::Value> {
    // 解析 ids：支持 { ids: [...] } 或顶层数组
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
    // 解析可选 clientRevs：{ id → rev }（rev 为字符串）
    let client_revs: std::collections::HashMap<String, String> = body
        .get("clientRevs")
        .and_then(|v| v.as_object())
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default();
    // diff 模式：前端传了 clientRevs 即按差异同步（命中省 body 带宽）。
    // 与 moka 进程内缓存无关——rev 读源文件现算，故不查 page_cache_enabled 开关。
    let diff_mode = !client_revs.is_empty();
    // 去重 + 去空
    let mut seen = std::collections::HashSet::new();
    let cleaned: Vec<String> = ids
        .into_iter()
        .filter(|s| !s.is_empty() && seen.insert(s.clone()))
        .collect();
    if cleaned.is_empty() {
        return Err(PortalError::bad_request("ids 不能为空"));
    }
    // 单次上限保护
    if cleaned.len() > MAX_BATCH {
        return Err(PortalError::bad_request(format!(
            "单次最多 {MAX_BATCH} 个页面 ID"
        )));
    }

    // domain 分桶：按命名空间域聚合，减少分片读取次数
    let mut by_domain: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    let mut errors: Vec<serde_json::Value> = Vec::new();
    for raw in &cleaned {
        match parse_page_namespace(raw) {
            Ok(ns) => by_domain.entry(ns.domain).or_default().push(raw.clone()),
            Err(e) => errors.push(json!({ "id": raw, "error": e.to_string() })),
        }
    }

    // 分片缓存：同域只读一次
    let mut shard_cache: std::collections::HashMap<String, Vec<serde_json::Value>> =
        std::collections::HashMap::new();
    let mut legacy_list: Option<Vec<serde_json::Value>> = None;
    let mut pages: Vec<serde_json::Value> = Vec::new();
    let mut revs = serde_json::Map::new();

    // 逐域逐 id 查找并读取
    for (domain, id_list) in by_domain {
        if !shard_cache.contains_key(&domain) {
            shard_cache.insert(domain.clone(), load_shard(&domain).await?);
        }
        let shard = shard_cache.get(&domain).cloned().unwrap_or_default();
        for id in id_list {
            // 先查分片，未命中回退 v1 列表
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
                    Ok(full) => {
                        // 方案2：rev 由 read_full_from_row 读源文件现算（不读索引行 rev）。
                        let actual_rev = full
                            .get("rev")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        revs.insert(id.clone(), json!(actual_rev));
                        // 差异模式：clientRevs 命中（rev 相等）则省略 body，仅出现在 revs 清单。
                        // 注：方案2下命中也要读源文件算 hash（无法避免），但省了 body 网络传输。
                        let hit = diff_mode
                            && client_revs.get(&id).map(|c| c == &actual_rev).unwrap_or(false);
                        if !hit {
                            pages.push(full);
                        }
                    }
                    Err(e) => {
                        errors.push(json!({ "id": id, "error": e.to_string() }))
                    }
                },
                None => errors.push(json!({ "id": id, "error": "页面不存在" })),
            }
        }
    }

    Ok(json!({ "pages": pages, "revs": revs, "errors": errors }))
}

// 注：索引重建（rebuild-index）接口已移除。方案改为读路径实时计算 rev（天然一致），
// 不再依赖索引行存储的 rev，故手动/AI 改源文件后无需重建索引——下次读取即感知变化。

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
