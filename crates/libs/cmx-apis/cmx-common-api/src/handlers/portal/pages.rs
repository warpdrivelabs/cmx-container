//! 表单页 / 原生页面 / HTML 页面 handler。

use axum::Json;
use axum::extract::{Path, Query};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;

use std::collections::BTreeMap;

use crate::middleware::CmxSvrContext;
use crate::{ApiResp, Result};

/// 页面 id → 属主服务键（`None` = 门户本地数据根）。
///
/// 前缀表与各引擎 F3 反代谓词一一对应
/// （cmx-model-proxy / cmx-mdm-proxy / flow-rpt-rule api proxy），新增属主时两处同步。
fn owner_service_of(id: &str) -> Option<&'static str> {
    if id.starts_with("portal.mdm.") {
        Some("mdm")
    } else if id.starts_with("portal.flow.") || id.starts_with("fi.cmxfico.gl.flow-") {
        Some("flow")
    } else if id.starts_with("portal.rules.") {
        Some("rules")
    } else if id.starts_with("portal.rpt.")
        || id.starts_with("portal.consol.")
        || id.starts_with("fi.cmxfico.gl.rpt-designer-")
        || id.starts_with("fi.cmxfico.gl.rpt-spreadjs-designer-")
    {
        Some("report")
    } else if id.starts_with("portal.model.") {
        Some("model")
    } else {
        None
    }
}

/// 委托身份头（引擎侧 HS256 验签语义，门户/引擎共享 jwt_secret，可安全透传）。
const DELEGATED_HEADER: &str = "x-delegated-user-token";

/// 平台出站服务凭证：`[service_auth].outgoing_api_key`（与 F3 反代注入同一来源；
/// 引擎 auth mw 命中 `X-API-Key` 即认服务身份）。
fn outgoing_api_key() -> Option<String> {
    cmx_utils::ConfigManager::try_global()
        .and_then(|cm| cm.get_string("service_auth.outgoing_api_key").ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// 构造发往属主引擎的请求头：服务身份 X-API-Key（出站凭证，缺省回退透传调用方的）
/// + 委托用户令牌。**不透传** `Authorization`（门户会话 Bearer 引擎不认，401 根因）。
fn forward_headers(headers: &HeaderMap) -> HeaderMap {
    let mut h = HeaderMap::new();
    match outgoing_api_key() {
        Some(key) => {
            h.insert(
                axum::http::HeaderName::from_static("x-api-key"),
                axum::http::HeaderValue::from_str(&key)
                    .expect("outgoing_api_key 含非法头字符"),
            );
        }
        None => {
            if let Some(v) = headers.get("x-api-key") {
                h.insert(axum::http::HeaderName::from_static("x-api-key"), v.clone());
            }
        }
    }
    if let Some(v) = headers.get(DELEGATED_HEADER)
        && let Ok(name) = axum::http::HeaderName::try_from(DELEGATED_HEADER)
    {
        h.insert(name, v.clone());
    }
    h
}

/// 从 batch 请求体提取 id 列表（兼容 `{ids:[...]}` / 顶层数组）。
fn batch_ids(body: &serde_json::Value) -> Vec<String> {
    let src = body
        .get("ids")
        .and_then(|v| v.as_array())
        .or_else(|| body.as_array());
    src.map(|a| {
        a.iter()
            .filter_map(|v| v.as_str().map(|s| s.trim().to_string()))
            .filter(|s| !s.is_empty())
            .collect()
    })
    .unwrap_or_default()
}

/// 远程批量取页：`POST {base}/api/{kind}-pages/batch`。成功返回 data 节点；失败返回摘要。
async fn fetch_remote_batch(
    kind: &str,
    key: &str,
    ids: &[String],
    fwd: &HeaderMap,
) -> std::result::Result<serde_json::Value, String> {
    let base = cmx_plugin::center_client::proxy_upstream(key)
        .and_then(|u| u.resolve())
        .ok_or_else(|| format!("服务 {key} 未配置或不可达"))?;
    let url = format!("{base}/api/{kind}-pages/batch");
    let cli = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?;
    let mut req = cli.post(&url).json(&json!({ "ids": ids }));
    for (k, v) in fwd.iter() {
        req = req.header(k, v);
    }
    let resp = req.send().await.map_err(|e| format!("请求 {url}: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("服务 {key} HTTP {}", resp.status()));
    }
    let val: serde_json::Value =
        resp.json().await.map_err(|e| format!("响应解析失败: {e}"))?;
    match val.get("code").and_then(|c| c.as_u64()) {
        Some(0) => Ok(val.get("data").cloned().unwrap_or(serde_json::Value::Null)),
        _ => Err(format!(
            "服务 {key} 业务错误: {}",
            val.get("msg").and_then(|m| m.as_str()).unwrap_or("未知")
        )),
    }
}

use serde_json::Value;
use serde_json::json;

/// 远程保存（F3-save）：`POST {base}/api/html-pages` 整包转发属主引擎。
///
/// body 原样透传（属主引擎侧 upsert）；成功返回其 `data` 节点（写后的行）。
async fn forward_remote_save(
    key: &str,
    body: &Value,
    fwd: &HeaderMap,
) -> Result<serde_json::Value> {
    let base = cmx_plugin::center_client::proxy_upstream(key)
        .and_then(|u| u.resolve())
        .ok_or_else(|| {
            cmx_api_types::Error::business_error(format!("服务 {key} 未配置或不可达"))
        })?;
    let url = format!("{base}/api/html-pages");
    let cli = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| cmx_api_types::Error::internal_error(format!("HTTP 客户端构建失败: {e}")))?;
    let mut req = cli.post(&url).json(body);
    for (k, v) in fwd.iter() {
        req = req.header(k, v);
    }
    let resp = req
        .send()
        .await
        .map_err(|e| cmx_api_types::Error::business_error(format!("请求 {url}: {e}")))?;
    if !resp.status().is_success() {
        return Err(cmx_api_types::Error::business_error(format!(
            "服务 {key} HTTP {}",
            resp.status()
        )));
    }
    let val: Value = resp
        .json()
        .await
        .map_err(|e| cmx_api_types::Error::business_error(format!("响应解析失败: {e}")))?;
    match val.get("code").and_then(|c| c.as_u64()) {
        Some(0) => Ok(val.get("data").cloned().unwrap_or(Value::Null)),
        _ => Err(cmx_api_types::Error::business_error(format!(
            "服务 {key} 业务错误: {}",
            val.get("msg").and_then(|m| m.as_str()).unwrap_or("未知")
        ))),
    }
}

/// 三段合并累积器（pages / revs / errors，native 与 html 同形态）。
#[derive(Default)]
struct MergedBatch {
    pages: Vec<Value>,
    revs: serde_json::Map<String, Value>,
    errors: Vec<Value>,
}

impl MergedBatch {
    /// 吸收门户本地结果（get_*_pages_by_ids 的输出）。
    fn absorb_local(&mut self, v: Value) {
        if let Some(a) = v.get("pages").and_then(|x| x.as_array()) {
            self.pages.extend(a.iter().cloned());
        }
        if let Some(m) = v.get("revs").and_then(|x| x.as_object()) {
            self.revs.extend(m.iter().map(|(k, v)| (k.clone(), v.clone())));
        }
        if let Some(a) = v.get("errors").and_then(|x| x.as_array()) {
            self.errors.extend(a.iter().cloned());
        }
    }

    /// 吸收引擎 native batch 结果（{items:[NativePageFull]}）。
    fn absorb_native_items(&mut self, ids: &[String], v: Value) {
        let items = v.get("items").and_then(|x| x.as_array()).cloned().unwrap_or_default();
        for it in items {
            if let (Some(id), Some(rev)) = (
                it.get("id").and_then(|x| x.as_str()),
                it.get("rev").and_then(|x| x.as_str()),
            ) {
                self.revs.insert(id.to_string(), json!(rev));
            }
            self.pages.push(it);
        }
        // 引擎未回的 id 视为不存在
        let got: Vec<&str> = self
            .pages
            .iter()
            .filter_map(|p| p.get("id").and_then(|x| x.as_str()))
            .collect();
        for id in ids {
            if !got.contains(&id.as_str()) {
                self.errors.push(json!({ "id": id, "error": "不存在" }));
            }
        }
    }

    /// 吸收引擎 html batch 结果（{pages,revs,errors} 同构直并）。
    fn absorb_remote(&mut self, ids: &[String], v: Value) {
        self.absorb_local(v);
        let got: Vec<&str> = self
            .pages
            .iter()
            .filter_map(|p| p.get("id").and_then(|x| x.as_str()))
            .collect();
        for id in ids {
            if !got.contains(&id.as_str())
                && !self.errors.iter().any(|e| e.get("id").and_then(|x| x.as_str()) == Some(id.as_str()))
            {
                self.errors.push(json!({ "id": id, "error": "不存在" }));
            }
        }
    }

    fn absorb_group_error(&mut self, key: &str, ids: &[String], msg: &str) {
        for id in ids {
            self.errors
                .push(json!({ "id": id, "error": format!("服务 {key} 不可用: {msg}") }));
        }
    }

    fn into_value(self) -> Value {
        json!({
            "pages": self.pages,
            "revs": Value::Object(self.revs),
            "errors": self.errors,
        })
    }
}

/// batch 聚合分发核心：按 id 归属拆分 → 本地读 + 并行反代属主服务 → 合并三段。
async fn batch_pages_fanout(
    kind: &'static str,
    headers: &HeaderMap,
    body: &Value,
) -> Result<Json<ApiResp<Value>>> {
    let ids = batch_ids(body);
    if ids.is_empty() {
        return Ok(Json(ApiResp::ok(json!({
            "pages": [], "revs": {}, "errors": [],
        }))));
    }

    // 按属主分组（保持请求顺序）
    let mut groups: BTreeMap<&'static str, Vec<String>> = BTreeMap::new();
    let mut local_ids = Vec::new();
    for id in &ids {
        match owner_service_of(id) {
            Some(k) => groups.entry(k).or_default().push(id.clone()),
            None => local_ids.push(id.clone()),
        }
    }

    let mut merged = MergedBatch::default();
    let fwd = forward_headers(headers);

    // 本地组
    if !local_ids.is_empty() {
        let mut lb = json!({ "ids": local_ids });
        if let Some(cr) = body.get("clientRevs") {
            lb["clientRevs"] = cr.clone();
        }
        let v = match kind {
            "native" => cmx_portal::pages::native::get_native_pages_by_ids(&lb).await?,
            _ => cmx_portal::pages::html::get_html_pages_by_ids(&lb).await?,
        };
        merged.absorb_local(v);
    }

    // 远程组并发
    let mut tasks = tokio::task::JoinSet::new();
    for (key, gids) in groups {
        let kind = kind.to_string();
        let fwd = fwd.clone();
        tasks.spawn(async move { (key, gids.clone(), fetch_remote_batch(&kind, key, &gids, &fwd).await) });
    }
    while let Some(res) = tasks.join_next().await {
        match res {
            Ok((_key, gids, Ok(v))) => {
                if kind == "native" {
                    merged.absorb_native_items(&gids, v);
                } else {
                    merged.absorb_remote(&gids, v);
                }
            }
            Ok((key, gids, Err(msg))) => merged.absorb_group_error(key, &gids, &msg),
            Err(e) => merged.errors.push(json!({ "error": format!("聚合任务失败: {e}") })),
        }
    }

    Ok(Json(ApiResp::ok(merged.into_value())))
}

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
    headers: HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    batch_pages_fanout("native", &headers, &body).await
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
/// `POST /api/html-pages` —— upsert（新建 / 更新）。**F3-save**：id 命中属主路由表
/// （`portal.model.*` → model 等）时整包反代属主引擎落盘（业务域页真源在各服务
/// assets 工作区）；其余落门户本地数据根。body：
///
/// ```json
/// {
///   "id": "页面 id",
///   "name": "页面名称",
///   "details": "页面描述",
///   "html": "HTML 源码（必填）",
///   "domain": "缺省由 id 命名空间推导（属主引擎侧三级回退：显式 > 既有行 > id 推导）",
///   "app": "同上",
///   "module": "同上",
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
    headers: HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    // F3-save：id 归属属主引擎 → 整包反代（body 原样透传，避免重序列化丢字段）
    let id = body
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if let Some(key) = owner_service_of(&id) {
        let saved = forward_remote_save(key, &body, &forward_headers(&headers)).await?;
        return Ok(Json(ApiResp::ok(saved)));
    }
    let input: cmx_portal::pages::html::HtmlPageInput = serde_json::from_value(body)
        .map_err(|e| cmx_api_types::Error::bad_request(format!("保存入参非法: {e}")))?;
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
    headers: HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    batch_pages_fanout("html", &headers, &body).await
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
