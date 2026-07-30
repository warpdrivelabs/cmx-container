//! 表单页 / 原生页面 / HTML 页面 handler。

use axum::Json;
use axum::extract::{Path, Query, State};
use serde::Deserialize;

use crate::app_state::CmxAppState;
use crate::middleware::CmxSvrContext;
use crate::{ApiResp, Result};

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
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<PageQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_portal::pages::form::list_form_pages_paged(q.page, q.page_size).await?,
    )))
}

/// `POST /api/form-pages` —— 保存。
pub async fn save_form_page(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Json(input): Json<cmx_portal::pages::form::FormPageInput>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_portal::pages::form::save_form_page(input).await?,
    )))
}

/// `GET /api/form-pages/:id` —— 单条。
pub async fn get_form_page(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Path(id): Path<String>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_portal::pages::form::get_form_page_by_id(&id).await?,
    )))
}

/// `GET /api/native-pages?page=&pageSize=` —— 分页列表。
pub async fn list_native_pages(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Query(q): Query<PageQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_portal::pages::native::list_native_pages_paged(q.page, q.page_size).await?,
    )))
}

/// `POST /api/native-pages` —— 保存。
pub async fn save_native_page(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Json(input): Json<cmx_portal::pages::native::NativePageInput>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_portal::pages::native::save_native_page(input).await?,
    )))
}

/// `POST /api/native-pages/batch` —— 批量取源码。
pub async fn batch_native_pages(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_portal::pages::native::get_native_pages_by_ids(&body).await?,
    )))
}

/// `GET /api/native-pages/:id` —— 单条（含源码）。
pub async fn get_native_page(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Path(id): Path<String>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let full = cmx_portal::pages::native::get_native_page_by_id(&id).await?;
    Ok(Json(ApiResp::ok(
        serde_json::to_value(full).map_err(cmx_portal::PortalError::from)?,
    )))
}

/// `GET /api/html-pages?page=&pageSize=&domain=&app=&module=&keyword=` —— 分页列表。
pub async fn list_html_pages(
    State(_s): State<CmxAppState>,
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
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Json(input): Json<cmx_portal::pages::html::HtmlPageInput>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_portal::pages::html::save_html_page(input).await?,
    )))
}

/// `POST /api/html-pages/batch` —— 批量取完整页面。
pub async fn batch_html_pages(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_portal::pages::html::get_html_pages_by_ids(&body).await?,
    )))
}

/// `GET /api/html-pages/:id` —— 单页（含 html）。
pub async fn get_html_page(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_c): CmxSvrContext,
    Path(id): Path<String>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(
        cmx_portal::pages::html::get_html_page_by_id(&id).await?,
    )))
}
