//! RulesProxyModule —— 平台→独立决策规则微服务的**反向代理壳**（对标 cmx-rpt-api 的 ReportProxyModule）。
//!
//! 规则引擎无进程内嵌壳（始终独立微服务）：`[center_client.urls].rules` 非空 → 挂本反代，平台
//! `/api/rules/*` 透明转发到远程 cmx-rule-server；空 → 平台无规则路由（规则页无法加载）。
//!
//! 规则微服务对外 URL 与平台一致（`/api/rules/v1/*`，无路径重写），故转发是恒等映射
//! `{rules_base}/api{原path}{query}`（与 report 同，不重写路径段）。
//!
//! 出站鉴权对齐平台既有三层（同 Flow/Report Proxy）：
//!   ① `X-API-Key`                      —— 平台服务身份（`[service_auth].outgoing_api_key`）
//!   ② `X-Delegated-User-Token: Bearer` —— 当前登录用户原始令牌（on-behalf-of）
//!   ③ `X-Request-Id`                   —— 链路追踪

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::any;
use axum::Router;

use cmx_api_core::routes::traits::ModuleRoutes;
use cmx_api_core::CmxAppState;

/// 反代模块：持远程 cmx-rule-server 基址 + 出站服务凭证 + 复用的 HTTP 客户端。
#[derive(Clone)]
pub struct RulesProxyModule {
    inner: std::sync::Arc<ProxyState>,
}

struct ProxyState {
    /// 远程规则微服务基址（如 `http://127.0.0.1:8094`），来自 `[center_client.urls].rules`。
    rules_base: String,
    /// 平台对外服务凭证（`[service_auth].outgoing_api_key`，注入 X-API-Key）。可空。
    api_key: Option<String>,
    client: reqwest::Client,
}

impl RulesProxyModule {
    /// 用远程基址 + 出站 API Key 构建。基址末尾多余 `/` 去掉。
    pub fn new(rules_base: impl Into<String>, api_key: Option<String>) -> Self {
        let rules_base = rules_base.into().trim_end_matches('/').to_string();
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_else(|e| {
                tracing::error!(error = %e, "RulesProxy 构建 reqwest 客户端失败，退回默认");
                reqwest::Client::new()
            });
        Self {
            inner: std::sync::Arc::new(ProxyState {
                rules_base,
                api_key,
                client,
            }),
        }
    }
}

impl ModuleRoutes for RulesProxyModule {
    fn routes(self) -> Router<CmxAppState> {
        // 覆盖规则前缀（根与子路径）。自持 State=proxy，故对 CmxAppState 是已 with_state 的子 Router。
        let proxy = self.inner;
        Router::new()
            .route("/rules", any(proxy_handler))
            .route("/rules/{*rest}", any(proxy_handler))
            .with_state(proxy)
    }

    fn prefix() -> &'static str {
        "rules"
    }

    fn module_name(&self) -> &'static str {
        "rules-proxy"
    }
}

/// 转发 handler：拼目标 URL → 注入三层鉴权 → 流式转发请求/响应。
async fn proxy_handler(State(px): State<std::sync::Arc<ProxyState>>, req: Request) -> Response {
    forward(&px, req).await
}

/// 复用的转发核：拼 `{rules_base}/api{path}{query}` → 注入三层鉴权 → 流式转发。
/// 被 `proxy_handler`（API 反代）与 `page_proxy_mw`（页面反代）共用。
async fn forward(px: &ProxyState, req: Request) -> Response {
    let method = req.method().clone();
    let uri = req.uri().clone();
    let headers = req.headers().clone();

    // 本进程收到的 path 已被外层 nest("/api") 剥去 `/api`（如 `/rules/v1/definitions`）。
    // 规则微服务用同名 URL，故恒等转发到 `{rules_base}/api{path}{query}`。
    let path = uri.path();
    let query = uri.query().map(|q| format!("?{q}")).unwrap_or_default();
    let target = format!("{}/api{path}{query}", px.rules_base);

    // 请求体 → reqwest stream（双向流式）。
    let body = req.into_body();
    let reqwest_body = reqwest::Body::wrap_stream(body.into_data_stream());

    let mut rb = px.client.request(method, &target).body(reqwest_body);
    rb = rb.headers(forward_request_headers(&headers));

    // 三层出站鉴权。
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
            tracing::error!(error = %e, %target, "RulesProxy 转发失败");
            (
                StatusCode::BAD_GATEWAY,
                axum::Json(serde_json::json!({
                    "code": 502,
                    "msg": format!("规则服务不可达: {e}")
                })),
            )
                .into_response()
        }
    }
}

/// 逐跳头（RFC 7230 §6.1）+ host：转发时剥掉。
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

/// 过滤请求头供转发（剥逐跳/host/content-length；其余原样）。
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

/// reqwest 响应 → axum 响应（状态 + 头 + 流式体；SSE 逐块透传）。
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
    let stream = resp.bytes_stream();
    let body = Body::from_stream(stream);
    let mut out = (status, body).into_response();
    *out.headers_mut() = headers;
    out
}

// ============================================================================
// 页面反代：native 页也「一芯双壳」——门户按 [center_client.urls].rules 把**规则拥有的**页面取页
// 请求反代到独立 cmx-rule-server（它自暴同款字节对齐 API）。
// ----------------------------------------------------------------------------
// native-pages 是**共享端点**（/api/native-pages/{id}），只有 `portal.rules.*` 属规则，其余是门户
// 自己的页。故用中间件按 id 归属判定：命中规则 → 转发 rules-server；未命中 → next.run 落回门户。
// ============================================================================

/// 判定一个前端页 id 是否属规则引擎（与 cmx-rulesengine/web 的清单一致）：`portal.rules.*`。
fn is_rules_owned_page(id: &str) -> bool {
    id.starts_with("portal.rules.")
}

/// 从 path 提取页面 id：`/native-pages/{id}`（已剥 `/api`）。batch/list 不拦截（含混合 id）。
fn page_id_of(path: &str) -> Option<&str> {
    let rest = path.strip_prefix("/native-pages/")?;
    if !rest.is_empty() && rest != "batch" && !rest.contains('/') {
        Some(rest)
    } else {
        None
    }
}

/// 页面反代中间件：规则拥有的单页取页请求 → 转发 rules-server；其余 → 落回门户 handler。
async fn page_proxy_mw(
    State(px): State<std::sync::Arc<ProxyState>>,
    req: Request,
    next: axum::middleware::Next,
) -> Response {
    if let Some(id) = page_id_of(req.uri().path())
        && is_rules_owned_page(id)
    {
        return forward(&px, req).await;
    }
    next.run(req).await
}

/// 给 api 路由叠加**规则页面反代**层：规则拥有的 native 单页取页请求转发到独立 cmx-rule-server，
/// 其余落回门户内嵌 handler。复用 `RulesProxyModule` 的远程基址 + 出站凭证 + HTTP 客户端。
pub fn with_rules_page_proxy(
    router: Router<CmxAppState>,
    rules_base: impl Into<String>,
    api_key: Option<String>,
) -> Router<CmxAppState> {
    let state = RulesProxyModule::new(rules_base, api_key).inner;
    router.layer(axum::middleware::from_fn_with_state(state, page_proxy_mw))
}
