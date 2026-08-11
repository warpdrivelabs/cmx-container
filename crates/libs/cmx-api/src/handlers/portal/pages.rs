//! 表单页 / 原生页面 / HTML 页面 handler。

use axum::Json;
use axum::extract::{Path, Query};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;

use crate::middleware::CmxSvrContext;
use crate::{ApiResp, Result};

/// `Cache-Control`：private + no-cache（每次 revalidate，但只在 rev 变了才传 body）。
const PAGE_CACHE_CONTROL: &str = "private, no-cache";

/// 解析请求的 `If-None-Match` 头（弱/强 ETag 均按裸值比对）。
fn if_none_match(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().trim_start_matches("W/").trim_matches('"').to_string())
}

/// 构造带 ETag / Cache-Control 的响应；`If-None-Match` 命中（rev 相等）则返回 304 空 body。
///
/// `rev` 为页面内容版本锚点（xxhash64 → 16 hex），同时作 ETag 值。
///
/// 注：ETag/304 是 HTTP 协议层缓存（省浏览器↔后端带宽），与 moka L1 进程内缓存（省磁盘 I/O）
/// 是两个独立维度。rev 由读路径实时算（不依赖 moka），故本函数**不受 `page_cache_enabled` 开关控制**——
/// 即使进程内缓存关闭，浏览器侧的 304 仍应正常生效。
fn render_with_etag(headers: &HeaderMap, rev: &str, body: serde_json::Value) -> Response {
    if let Some(client_rev) = if_none_match(headers)
        && !rev.is_empty()
        && client_rev == rev
    {
        // 命中：304 空 body，仍带 ETag/Cache-Control 供下次校验。
        return (
            StatusCode::NOT_MODIFIED,
            [(header::ETAG, HeaderValue::from_str(format!("\"{rev}\"").as_str()).unwrap())],
            [(header::CACHE_CONTROL, HeaderValue::from_static(PAGE_CACHE_CONTROL))],
            "",
        )
            .into_response();
    }
    let etag = HeaderValue::from_str(format!("\"{rev}\"").as_str()).unwrap();
    let resp = Json(ApiResp::ok(body)).into_response();
    let mut resp = resp;
    resp.headers_mut().insert(header::ETAG, etag);
    resp.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(PAGE_CACHE_CONTROL),
    );
    resp
}

#[derive(Debug, Deserialize)]
pub struct PageQuery {
    #[serde(default)]
    pub page: Option<i64>,
    #[serde(default, rename = "pageSize", alias = "page_size")]
    pub page_size: Option<i64>,
}

/// html-pages 列表查询：分页 + keyword 搜索 + domain/app/module 过滤。
#[derive(Debug, Deserialize)]
pub struct HtmlListQuery {
    #[serde(default)]
    pub page: Option<i64>,
    #[serde(default, rename = "pageSize", alias = "page_size")]
    pub page_size: Option<i64>,
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub app: Option<String>,
    #[serde(default)]
    pub module: Option<String>,
    /// 关键词：对 id/name/details 做不区分大小写的包含匹配。
    #[serde(default)]
    pub keyword: Option<String>,
}

/// `GET /api/form-pages?page=&pageSize=` —— 分页列表。
pub async fn list_form_pages(
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<PageQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_portal::pages::form::list_form_pages_paged(q.page, q.page_size).await?,
    )))
}

/// `POST /api/form-pages` —— 保存。
pub async fn save_form_page(
    CmxSvrContext(_c): CmxSvrContext,
    Json(input): Json<cmx_portal::pages::form::FormPageInput>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_portal::pages::form::save_form_page(input).await?,
    )))
}

/// `GET /api/form-pages/:id` —— 单条。
pub async fn get_form_page(
    CmxSvrContext(_c): CmxSvrContext,
    Path(id): Path<String>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_portal::pages::form::get_form_page_by_id(&id).await?,
    )))
}

/// `GET /api/native-pages?page=&pageSize=` —— 分页列表。
pub async fn list_native_pages(
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<PageQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_portal::pages::native::list_native_pages_paged(q.page, q.page_size).await?,
    )))
}

/// `POST /api/native-pages` —— 保存。
pub async fn save_native_page(
    CmxSvrContext(_c): CmxSvrContext,
    Json(input): Json<cmx_portal::pages::native::NativePageInput>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_portal::pages::native::save_native_page(input).await?,
    )))
}

/// `POST /api/native-pages/batch` —— 批量取源码。
pub async fn batch_native_pages(
    CmxSvrContext(_c): CmxSvrContext,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_portal::pages::native::get_native_pages_by_ids(&body).await?,
    )))
}

/// `GET /api/native-pages/:id` —— 单条（含源码）。
///
/// 支持 `If-None-Match` → 304（rev 命中）；响应带 `ETag` / `Cache-Control`。
pub async fn get_native_page(
    CmxSvrContext(_c): CmxSvrContext,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Response> {
    let full = cmx_portal::pages::native::get_native_page_by_id(&id).await?;
    let rev = full.rev.clone();
    let body = serde_json::to_value(full).map_err(cmx_portal::PortalError::from)?;
    Ok(render_with_etag(&headers, &rev, body))
}

/// `GET /api/html-pages?page=&pageSize=&domain=&app=&module=&keyword=` —— 分页列表。
pub async fn list_html_pages(
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<HtmlListQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let doc = cmx_portal::pages::html::list_html_pages_paged(
        q.page,
        q.page_size,
        q.domain.as_deref(),
        q.app.as_deref(),
        q.module.as_deref(),
        q.keyword.as_deref(),
    )
    .await?;
    Ok(Json(ApiResp::ok(doc)))
}

/// `POST /api/html-pages` —— 保存。
pub async fn save_html_page(
    CmxSvrContext(_c): CmxSvrContext,
    Json(input): Json<cmx_portal::pages::html::HtmlPageInput>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_portal::pages::html::save_html_page(input).await?,
    )))
}

/// `POST /api/html-pages/batch` —— 批量取完整页面。
pub async fn batch_html_pages(
    CmxSvrContext(_c): CmxSvrContext,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_portal::pages::html::get_html_pages_by_ids(&body).await?,
    )))
}

/// `GET /api/html-pages/:id` —— 单页（含 html）。
///
/// 支持 `If-None-Match` → 304（rev 命中）；响应带 `ETag` / `Cache-Control`。
pub async fn get_html_page(
    CmxSvrContext(_c): CmxSvrContext,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Response> {
    let page = cmx_portal::pages::html::get_html_page_by_id(&id).await?;
    let rev = page
        .get("rev")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    Ok(render_with_etag(&headers, &rev, page))
}
