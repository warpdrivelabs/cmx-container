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

/// 表单页 / 原生页面列表分页参数。
#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct PageQuery {
    /// 页码（可选）。
    #[serde(default)]
    pub page: Option<i64>,
    /// 每页条数（可选；query key `pageSize`，兼容 `page_size`）。
    #[serde(default, rename = "pageSize", alias = "page_size")]
    pub page_size: Option<i64>,
}

/// html-pages 列表查询：分页 + keyword 搜索 + domain/app/module 过滤。
#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct HtmlListQuery {
    /// 页码（可选）。
    #[serde(default)]
    pub page: Option<i64>,
    /// 每页条数（可选；query key `pageSize`，兼容 `page_size`）。
    #[serde(default, rename = "pageSize", alias = "page_size")]
    pub page_size: Option<i64>,
    /// 域过滤（可选）。
    #[serde(default)]
    pub domain: Option<String>,
    /// 应用过滤（可选；query key `app`，兼容 `application`）。
    #[serde(default)]
    pub app: Option<String>,
    /// 模块过滤（可选）。
    #[serde(default)]
    pub module: Option<String>,
    /// 关键词：对 id/name/details 做不区分大小写的包含匹配。
    #[serde(default)]
    pub keyword: Option<String>,
}

/// 列出表单页。
///
/// `GET /api/form-pages?page=&pageSize=` —— 分页列表（索引信息，不含 form JSON 正文）。
#[utoipa::path(
    get,
    path = "/api/form-pages",
    params(PageQuery),
    responses(
        (status = 200, description = "分页列表（items / total 等）", body = ApiResp<serde_json::Value>)
    ),
    tag = "门户接口"
)]
pub async fn list_form_pages(
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<PageQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_portal::pages::form::list_form_pages_paged(q.page, q.page_size).await?,
    )))
}

/// 保存表单页。
///
/// `POST /api/form-pages` —— upsert（新建 / 更新）。body：
///
/// ```json
/// {
///   "id": "页面 id（字母数字._-，1-128）",
///   "name": "页面名称",
///   "details": "页面描述",
///   "form": "CMX 表单 JSON 字符串（必填）"
/// }
/// ```
#[utoipa::path(
    post,
    path = "/api/form-pages",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "保存后的页面记录", body = ApiResp<serde_json::Value>)
    ),
    tag = "门户接口"
)]
pub async fn save_form_page(
    CmxSvrContext(_c): CmxSvrContext,
    Json(input): Json<cmx_portal::pages::form::FormPageInput>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_portal::pages::form::save_form_page(input).await?,
    )))
}

/// 取单个表单页。
///
/// `GET /api/form-pages/{id}` —— 单条（含 form JSON）。
#[utoipa::path(
    get,
    path = "/api/form-pages/{id}",
    params(
        ("id" = String, Path, description = "表单页 id")
    ),
    responses(
        (status = 200, description = "表单页完整记录（含 form JSON）", body = ApiResp<serde_json::Value>)
    ),
    tag = "门户接口"
)]
pub async fn get_form_page(
    CmxSvrContext(_c): CmxSvrContext,
    Path(id): Path<String>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_portal::pages::form::get_form_page_by_id(&id).await?,
    )))
}

/// 列出原生页面。
///
/// `GET /api/native-pages?page=&pageSize=` —— 分页列表（索引信息，不含源码）。
#[utoipa::path(
    get,
    path = "/api/native-pages",
    params(PageQuery),
    responses(
        (status = 200, description = "分页列表（items / total 等）", body = ApiResp<serde_json::Value>)
    ),
    tag = "门户接口"
)]
pub async fn list_native_pages(
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<PageQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_portal::pages::native::list_native_pages_paged(q.page, q.page_size).await?,
    )))
}

/// 保存原生页面。
///
/// `POST /api/native-pages` —— upsert（新建 / 更新）。body：
///
/// ```json
/// {
///   "id": "页面 id（点分命名空间）",
///   "name": "页面名称",
///   "details": "页面描述",
///   "sourceType": "js | html",
///   "source": "源码文本（必填）",
///   "relPath": "源文件相对路径（缺省由 id + sourceType 推导）"
/// }
/// ```
#[utoipa::path(
    post,
    path = "/api/native-pages",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "保存后的页面记录", body = ApiResp<serde_json::Value>)
    ),
    tag = "门户接口"
)]
pub async fn save_native_page(
    CmxSvrContext(_c): CmxSvrContext,
    Json(input): Json<cmx_portal::pages::native::NativePageInput>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_portal::pages::native::save_native_page(input).await?,
    )))
}

/// 批量取原生页面。
///
/// `POST /api/native-pages/batch` —— 按 id 批量取页面（含源码）。body 为
/// `{ "ids": ["id1", "id2"] }` 或顶层字符串数组 `["id1", "id2"]`。
#[utoipa::path(
    post,
    path = "/api/native-pages/batch",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "批量页面（含源码）", body = ApiResp<serde_json::Value>)
    ),
    tag = "门户接口"
)]
pub async fn batch_native_pages(
    CmxSvrContext(_c): CmxSvrContext,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_portal::pages::native::get_native_pages_by_ids(&body).await?,
    )))
}

/// 取单个原生页面。
///
/// `GET /api/native-pages/{id}` —— 单条（含源码）。支持 `If-None-Match` → 304
/// （rev 命中）；响应带 `ETag` / `Cache-Control`（private, no-cache）。
#[utoipa::path(
    get,
    path = "/api/native-pages/{id}",
    params(
        ("id" = String, Path, description = "原生页面 id")
    ),
    responses(
        (status = 200, description = "页面完整记录（含源码）；响应头带 ETag / Cache-Control", body = ApiResp<serde_json::Value>),
        (status = 304, description = "If-None-Match 命中（rev 未变），空 body")
    ),
    tag = "门户接口"
)]
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

/// 列出 HTML 页面。
///
/// `GET /api/html-pages` —— 分页列表（索引信息，不含 html 正文），支持 keyword
/// 搜索与 domain / app / module 过滤。
#[utoipa::path(
    get,
    path = "/api/html-pages",
    params(HtmlListQuery),
    responses(
        (status = 200, description = "分页列表（items / total 等）", body = ApiResp<serde_json::Value>)
    ),
    tag = "门户接口"
)]
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

/// 保存 HTML 页面。
///
/// `POST /api/html-pages` —— upsert（新建 / 更新，写源文件 + 列表双写）。body：
///
/// ```json
/// {
///   "id": "页面 id",
///   "name": "页面名称",
///   "details": "页面描述",
///   "html": "HTML 源码（必填）",
///   "domain": "缺省由 id 命名空间推导",
///   "app": "缺省由 id 命名空间推导",
///   "module": "缺省由 id 命名空间推导",
///   "doc": "绑定的单据模块编码 moduleCode（可选）"
/// }
/// ```
#[utoipa::path(
    post,
    path = "/api/html-pages",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "保存后的页面记录", body = ApiResp<serde_json::Value>)
    ),
    tag = "门户接口"
)]
pub async fn save_html_page(
    CmxSvrContext(_c): CmxSvrContext,
    Json(input): Json<cmx_portal::pages::html::HtmlPageInput>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_portal::pages::html::save_html_page(input).await?,
    )))
}

/// 批量取 HTML 页面。
///
/// `POST /api/html-pages/batch` —— 按 id 批量取完整页面（含 html）。body 为
/// `{ "ids": ["id1", "id2"] }` 或顶层字符串数组 `["id1", "id2"]`。
#[utoipa::path(
    post,
    path = "/api/html-pages/batch",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "批量页面（含 html）", body = ApiResp<serde_json::Value>)
    ),
    tag = "门户接口"
)]
pub async fn batch_html_pages(
    CmxSvrContext(_c): CmxSvrContext,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_portal::pages::html::get_html_pages_by_ids(&body).await?,
    )))
}

/// 取单个 HTML 页面。
///
/// `GET /api/html-pages/{id}` —— 单页（含 html）。支持 `If-None-Match` → 304
/// （rev 命中）；响应带 `ETag` / `Cache-Control`（private, no-cache）。
#[utoipa::path(
    get,
    path = "/api/html-pages/{id}",
    params(
        ("id" = String, Path, description = "HTML 页面 id")
    ),
    responses(
        (status = 200, description = "页面完整记录（含 html）；响应头带 ETag / Cache-Control", body = ApiResp<serde_json::Value>),
        (status = 304, description = "If-None-Match 命中（rev 未变），空 body")
    ),
    tag = "门户接口"
)]
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
