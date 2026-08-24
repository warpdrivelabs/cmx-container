//! 页面只读投递路由：native + html 六端点，泛型错误映射保各引擎历史错误体。

use axum::Json;
use axum::extract::{Path as AxPath, Query, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use serde::Deserialize;
use serde_json::{Value, json};

use cmx_api_types::ApiResp;

use super::config::{HtmlLayout, PageServeConfig};
use super::error::PageServeError;
use super::loader::{
    HtmlRow, load_html_full, load_native_full, read_html_rows, read_index, source_type_from_rel,
};

/// 页面只读路由装配入口。
///
/// # Arguments
///
/// * `cfg` - 页面配置（目录与 html 开关），经 State 注入 handler。
///
/// # Returns
///
/// 返回挂载于 `/api` 下的只读路由：native 三端点恒注册；html 三端点按
/// `cfg.html` 开关注册。`E` 为调用方错误类型（渲染历史错误体）：
/// mdm/model/report 传 `cmx_api_types::Error`，rule/flow 传各自 FlowError/RuleError。
pub fn frontend_pages_routes<S, E>(cfg: PageServeConfig) -> axum::Router<S>
where
    S: Clone + Send + Sync + 'static,
    E: IntoResponse + From<PageServeError> + 'static,
{
    let builder = axum::Router::new()
        .route("/native-pages", get(list_native_pages::<E>))
        .route("/native-pages/batch", post(batch_native_pages::<E>))
        .route("/native-pages/{id}", get(get_native_page::<E>));
    let builder = match cfg.html {
        HtmlLayout::Disabled => builder,
        HtmlLayout::ShardedV2 => builder
            .route("/html-pages", get(list_html_pages::<E>))
            .route("/html-pages/batch", post(batch_html_pages::<E>))
            .route("/html-pages/{id}", get(get_html_page::<E>)),
    };
    builder.with_state(cfg)
}

// ============================================================================
// native-pages handlers
// ============================================================================

/// 分页查询参数。
#[derive(Debug, Deserialize)]
pub struct PageQuery {
    /// 页码（从 1 起，缺省 1）。
    #[serde(default)]
    pub page: Option<u32>,
    /// 每页条数（缺省 50）。
    #[serde(default, rename = "pageSize")]
    pub page_size: Option<u32>,
}

/// 批量请求体。
#[derive(Debug, Deserialize)]
pub struct BatchReq {
    /// 页面 id 列表。
    #[serde(default)]
    pub ids: Vec<String>,
}

/// `GET /native-pages?page=&pageSize=` —— 分页列表（不含源码，对齐门户 list 前缀契约）。
pub(crate) async fn list_native_pages<E>(
    State(cfg): State<PageServeConfig>,
    Query(q): Query<PageQuery>,
) -> Result<Json<ApiResp<Value>>, E>
where
    E: From<PageServeError>,
{
    let idx = read_index(&cfg.native_dir);
    let total = idx.len();
    let page = q.page.unwrap_or(1).max(1) as usize;
    let size = q.page_size.unwrap_or(50).max(1) as usize;
    let start = (page - 1) * size;
    let items: Vec<Value> = idx
        .iter()
        .skip(start)
        .take(size)
        .map(|e| {
            json!({
                "id": e.id, "name": e.name, "details": e.details,
                "sourceType": if e.source_type.is_empty() { source_type_from_rel(&e.rel_path) } else { e.source_type.clone() },
                "relPath": e.rel_path,
            })
        })
        .collect();
    Ok(Json(ApiResp::ok(json!({
        "items": items, "total": total, "page": page, "pageSize": size,
    }))))
}

/// `POST /native-pages/batch` —— 批量取源码（body `{ids:[]}`）。单条失败静默跳过。
pub(crate) async fn batch_native_pages<E>(
    State(cfg): State<PageServeConfig>,
    Json(req): Json<BatchReq>,
) -> Result<Json<ApiResp<Value>>, E>
where
    E: From<PageServeError>,
{
    let idx = read_index(&cfg.native_dir);
    let mut items = Vec::new();
    for id in &req.ids {
        if let Some(e) = idx.iter().find(|e| &e.id == id)
            && let Ok(full) = load_native_full(&cfg.native_dir, e)
        {
            items.push(serde_json::to_value(full).unwrap_or(Value::Null));
        }
    }
    Ok(Json(ApiResp::ok(json!({ "items": items }))))
}

/// `GET /native-pages/{id}` —— 单条含源码。
pub(crate) async fn get_native_page<E>(
    State(cfg): State<PageServeConfig>,
    AxPath(id): AxPath<String>,
) -> Result<Json<ApiResp<crate::pages::native::NativePageFull>>, E>
where
    E: From<PageServeError>,
{
    let idx = read_index(&cfg.native_dir);
    let e = idx
        .iter()
        .find(|e| e.id == id)
        .ok_or_else(|| PageServeError::NotFound(format!("native page 不存在: {id}")))?;
    Ok(Json(ApiResp::ok(load_native_full(&cfg.native_dir, e)?)))
}

// ============================================================================
// html-pages handlers（v2 分片）
// ============================================================================

/// html 分页列表查询参数。
#[derive(Debug, Deserialize)]
pub struct HtmlListQuery {
    /// 页码（从 1 起，缺省 1）。
    #[serde(default)]
    pub page: Option<u32>,
    /// 每页条数（缺省 50）。
    #[serde(default, rename = "pageSize")]
    pub page_size: Option<u32>,
    /// 按 domain 精确过滤。
    #[serde(default)]
    pub domain: Option<String>,
    /// 按 app 精确过滤。
    #[serde(default)]
    pub app: Option<String>,
    /// 按 module 精确过滤。
    #[serde(default)]
    pub module: Option<String>,
    /// 关键字（匹配 id 或 name 包含）。
    #[serde(default)]
    pub keyword: Option<String>,
}

/// `GET /html-pages?...` —— v2 分片索引分页列表（不含 html）。
pub(crate) async fn list_html_pages<E>(
    State(cfg): State<PageServeConfig>,
    Query(q): Query<HtmlListQuery>,
) -> Result<Json<ApiResp<Value>>, E>
where
    E: From<PageServeError>,
{
    let rows = read_html_rows(&cfg.html_dir);
    let filtered: Vec<&HtmlRow> = rows
        .iter()
        .filter(|r| {
            q.domain.as_ref().map(|d| &r.domain == d).unwrap_or(true)
                && q.app.as_ref().map(|a| &r.app == a).unwrap_or(true)
                && q.module.as_ref().map(|m| &r.module == m).unwrap_or(true)
                && q.keyword.as_ref().map(|k| r.id.contains(k) || r.name.contains(k)).unwrap_or(true)
        })
        .collect();
    let total = filtered.len();
    let page = q.page.unwrap_or(1).max(1) as usize;
    let size = q.page_size.unwrap_or(50).max(1) as usize;
    let start = (page - 1) * size;
    let items: Vec<Value> = filtered
        .iter()
        .skip(start)
        .take(size)
        .map(|r| {
            json!({
                "id": r.id, "name": r.name, "details": r.details,
                "domain": r.domain, "app": r.app, "module": r.module, "relPath": r.rel_path,
            })
        })
        .collect();
    Ok(Json(ApiResp::ok(json!({
        "items": items, "total": total, "page": page, "pageSize": size,
    }))))
}

/// `POST /html-pages/batch` —— 批量取页面。返回 `{pages,revs,errors}`：单条失败记入 errors 不阻断。
pub(crate) async fn batch_html_pages<E>(
    State(cfg): State<PageServeConfig>,
    Json(req): Json<BatchReq>,
) -> Result<Json<ApiResp<Value>>, E>
where
    E: From<PageServeError>,
{
    let rows = read_html_rows(&cfg.html_dir);
    let mut pages = Vec::new();
    let mut revs = serde_json::Map::new();
    let mut errors = Vec::new();
    for id in &req.ids {
        match rows.iter().find(|r| &r.id == id) {
            Some(r) => match load_html_full(&cfg.html_dir, r) {
                Ok(full) => {
                    if let Some(rev) = full.get("rev").and_then(|v| v.as_str()) {
                        revs.insert(id.clone(), Value::String(rev.to_string()));
                    }
                    pages.push(full);
                }
                Err(_) => errors.push(json!({ "id": id, "error": "源码缺失" })),
            },
            None => errors.push(json!({ "id": id, "error": "不存在" })),
        }
    }
    Ok(Json(ApiResp::ok(json!({ "pages": pages, "revs": revs, "errors": errors }))))
}

/// `GET /html-pages/{id}` —— 单页含 html。
pub(crate) async fn get_html_page<E>(
    State(cfg): State<PageServeConfig>,
    AxPath(id): AxPath<String>,
) -> Result<Json<ApiResp<Value>>, E>
where
    E: From<PageServeError>,
{
    let rows = read_html_rows(&cfg.html_dir);
    let r = rows
        .iter()
        .find(|r| r.id == id)
        .ok_or_else(|| PageServeError::NotFound(format!("html page 不存在: {id}")))?;
    Ok(Json(ApiResp::ok(load_html_full(&cfg.html_dir, r)?)))
}
