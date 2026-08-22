//! ReportProxyModule —— 平台→独立报表微服务的**反向代理壳**（对标 [`cmx_flow_api`] 的 FlowProxyModule）。
//!
//! 「后端一芯双壳」：`ReportModule`（进程内嵌，`/api/report-design/*` 由本进程 handler 处理）
//! ↔ `ReportProxyModule`（引擎在**远程独立 cmx-rpt-server**，`/api/report-design/*` 透明转发到它）。
//! 二者对 web-server 是同一个 `impl ModuleRoutes` 契约、同一批报表前缀——**前端零改**（浏览器仍
//! 请求同源 `/api/report-design/...`），切换只看 `[center_client.urls].report` 配没配。
//!
//! 与 flow 的差异：报表微服务的对外 URL 与平台**完全一致**（`/api/report-design/*`、
//! `/api/report-source-bindings*`、`/api/rpt/compute`，**无 `/v1` 升级**），故转发路径是恒等映射
//! `{report_base}/api{原path}{query}`——比 flow 更简单，不重写路径段。
//!
//! 出站鉴权对齐平台既有三层（同 FlowProxy / `remote_importers::apply_auth_headers`）：
//!   ① `X-API-Key`                      —— 平台服务身份（`[service_auth].outgoing_api_key`）
//!   ② `X-Delegated-User-Token: Bearer` —— 当前登录用户原始令牌（on-behalf-of）
//!   ③ `X-Request-Id`                   —— 链路追踪

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::any;
use axum::Router;

use cmx_api_core::CmxAppState;
use cmx_api_core::routes::traits::ModuleRoutes;

/// 反代模块：持远程 cmx-rpt-server 基址 + 出站服务凭证 + 复用的 HTTP 客户端。
#[derive(Clone)]
pub struct ReportProxyModule {
    inner: std::sync::Arc<ProxyState>,
}

struct ProxyState {
    /// 远程报表微服务基址（如 `http://127.0.0.1:8092`），来自 `[center_client.urls].report`。
    report_base: String,
    /// 平台对外服务凭证（`[service_auth].outgoing_api_key`，注入 X-API-Key）。可空（不注入）。
    api_key: Option<String>,
    client: reqwest::Client,
}

impl ReportProxyModule {
    /// 用远程基址 + 出站 API Key 构建。基址末尾多余 `/` 去掉（拼接时统一补）。
    pub fn new(report_base: impl Into<String>, api_key: Option<String>) -> Self {
        let report_base = report_base.into().trim_end_matches('/').to_string();
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_else(|e| {
                tracing::error!(error = %e, "ReportProxy 构建 reqwest 客户端失败，退回默认");
                reqwest::Client::new()
            });
        Self {
            inner: std::sync::Arc::new(ProxyState {
                report_base,
                api_key,
                client,
            }),
        }
    }
}

impl ModuleRoutes for ReportProxyModule {
    fn routes(self) -> Router<CmxAppState> {
        // 覆盖报表三前缀（根与子路径）。自持 State=proxy，故对 CmxAppState 是一个已 with_state 的
        // 子 Router，merge 进主路由不影响主 state。
        let proxy = self.inner;
        Router::new()
            .route("/report-design", any(proxy_handler))
            .route("/report-design/{*rest}", any(proxy_handler))
            .route("/report-source-bindings", any(proxy_handler))
            .route("/report-source-bindings/{*rest}", any(proxy_handler))
            .route("/rpt/{*rest}", any(proxy_handler))
            // 合并报表:方案/范围/个别数/规则/往来录入 + 运行合并/对账 + 工作底稿/分类账查询。
            .route("/consol/{*rest}", any(proxy_handler))
            .with_state(proxy)
    }

    fn prefix() -> &'static str {
        "report"
    }

    fn module_name(&self) -> &'static str {
        "report-proxy"
    }
}

/// 转发 handler：拼目标 URL → 注入三层鉴权 → 流式转发请求/响应。
async fn proxy_handler(State(px): State<std::sync::Arc<ProxyState>>, req: Request) -> Response {
    forward(&px, req).await
}

/// 复用的转发核：拼 `{report_base}/api{path}{query}` → 注入三层鉴权 → 流式转发。
/// 被 `proxy_handler`（API 反代）与 `page_proxy_mw`（页面反代）共用。
async fn forward(px: &ProxyState, req: Request) -> Response {
    let method = req.method().clone();
    let uri = req.uri().clone();
    let headers = req.headers().clone();

    // 本进程收到的 path 已被外层 nest("/api") 剥去 `/api`（如 `/report-design/overview`）。
    // 报表微服务用同名 URL，故恒等转发到 `{report_base}/api{path}{query}`。
    let path = uri.path();
    let query = uri.query().map(|q| format!("?{q}")).unwrap_or_default();
    let target = format!("{}/api{path}{query}", px.report_base);

    // 请求体 → reqwest stream（双向流式，避免整体缓冲）。
    let body = req.into_body();
    let reqwest_body = reqwest::Body::wrap_stream(body.into_data_stream());

    let mut rb = px.client.request(method, &target).body(reqwest_body);

    // 透传原请求头（除逐跳/host，避免污染）。
    rb = rb.headers(forward_request_headers(&headers));

    // 三层出站鉴权（对齐 FlowProxy / remote_importers::apply_auth_headers）。
    if let Some(key) = &px.api_key {
        rb = rb.header("X-API-Key", key);
    }
    if let Some(user_jwt) = cmx_traits::auth::context_scope::current_original_token() {
        rb = rb.header("X-Delegated-User-Token", format!("Bearer {user_jwt}"));
    }
    if let Some(rid) = cmx_traits::auth::context_scope::current_request_id() {
        rb = rb.header("X-Request-Id", rid);
    }

    match rb.send().await {
        Ok(resp) => build_response(resp),
        Err(e) => {
            tracing::error!(error = %e, %target, "ReportProxy 转发失败");
            (
                StatusCode::BAD_GATEWAY,
                axum::Json(serde_json::json!({
                    "code": 502,
                    "msg": format!("报表服务不可达: {e}")
                })),
            )
                .into_response()
        }
    }
}

/// 逐跳头（RFC 7230 §6.1）+ host：转发时剥掉，由 reqwest/目标自定。
fn is_hop_by_hop(name: &HeaderName) -> bool {
    matches!(
        name.as_str(),
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
            | "host"
            | "content-length"
    )
}

/// 过滤请求头供转发（剥逐跳/host/content-length；其余原样，含 Authorization/Accept/Content-Type）。
fn forward_request_headers(src: &HeaderMap) -> HeaderMap {
    let mut out = HeaderMap::new();
    for (k, v) in src.iter() {
        if is_hop_by_hop(k) {
            continue;
        }
        out.insert(k.clone(), v.clone());
    }
    out
}

/// reqwest 响应 → axum 响应（状态 + 头 + 流式体）。
fn build_response(resp: reqwest::Response) -> Response {
    let status = StatusCode::from_u16(resp.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let mut headers = HeaderMap::new();
    for (k, v) in resp.headers().iter() {
        let name = HeaderName::from_bytes(k.as_ref());
        let val = HeaderValue::from_bytes(v.as_bytes());
        if let (Ok(name), Ok(val)) = (name, val) {
            if is_hop_by_hop(&name) {
                continue;
            }
            headers.insert(name, val);
        }
    }
    // 流式体（SSE 也走这条：content-type text/event-stream 保留，逐块透传）。
    let stream = resp.bytes_stream();
    let body = Body::from_stream(stream);
    let mut out = (status, body).into_response();
    *out.headers_mut() = headers;
    out
}

// ============================================================================
// 页面反代（F3a）：前端页 native/html 也「一芯双壳」——门户按 [center_client.urls].report
// 把**报表拥有的**页面取页请求反代到独立 cmx-rpt-server（它自暴同款字节对齐 API）。
// ----------------------------------------------------------------------------
// 与 API 反代的差异：native/html-pages 是**共享端点**（/api/native-pages/{id}），只有**部分 id**
// 属报表，其余是门户自己的页。故不能整前缀反代，改用**中间件按 id 归属判定**：
//   - 命中报表拥有的 id → 转发到 report-server；
//   - 未命中           → next.run(req) 落回门户自己的 handler（内嵌页零改）。
// 这样门户菜单打开报表页 → 浏览器 GET /api/native-pages/portal.rpt.designer → 命中反代 →
// report-server 返回逐字节一致的页面源（rev 一致，ETag/缓存不错位），shell 零感知。
// ============================================================================

/// 判定一个前端页 id 是否属报表（与 cmx-report/web 的清单一致）：
///   native：`portal.rpt.*`
///   html  ：`fi.cmxfico.gl.rpt-designer-*`、`fi.cmxfico.gl.rpt-spreadjs-designer-*`
fn is_report_owned_page(id: &str) -> bool {
    id.starts_with("portal.rpt.")
        || id.starts_with("portal.consol.")
        || id.starts_with("fi.cmxfico.gl.rpt-designer-")
        || id.starts_with("fi.cmxfico.gl.rpt-spreadjs-designer-")
}

/// 从 path 提取页面 id：`/native-pages/{id}` 或 `/html-pages/{id}`（已剥 `/api`）。
/// batch/list（`/native-pages/batch`、`/native-pages`）不在此拦截（含混合 id，留门户聚合）。
fn page_id_of(path: &str) -> Option<&str> {
    for pfx in ["/native-pages/", "/html-pages/"] {
        if let Some(rest) = path.strip_prefix(pfx) {
            if !rest.is_empty() && rest != "batch" && !rest.contains('/') {
                return Some(rest);
            }
        }
    }
    None
}

/// 页面反代中间件：报表拥有的单页取页请求 → 转发 report-server；其余 → 落回门户 handler。
/// 由平台在 `report_remote_base()` 非空时对 api 路由 `layer` 之。
async fn page_proxy_mw(
    State(px): State<std::sync::Arc<ProxyState>>,
    req: Request,
    next: axum::middleware::Next,
) -> Response {
    if let Some(id) = page_id_of(req.uri().path()) {
        if is_report_owned_page(id) {
            return forward(&px, req).await;
        }
    }
    next.run(req).await
}

/// 给 api 路由叠加**报表页面反代**层：报表拥有的 native/html 单页取页请求转发到独立 cmx-rpt-server，
/// 其余落回门户内嵌 handler。复用 `ReportProxyModule` 的远程基址 + 出站凭证 + HTTP 客户端。
/// 平台 `merge_report` 在 `report_remote_base()` 非空时调它（返回叠层后的同类型路由）。
pub fn with_report_page_proxy(
    router: Router<CmxAppState>,
    report_base: impl Into<String>,
    api_key: Option<String>,
) -> Router<CmxAppState> {
    let state = ReportProxyModule::new(report_base, api_key).inner;
    router.layer(axum::middleware::from_fn_with_state(state, page_proxy_mw))
}
