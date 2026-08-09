//! FlowProxyModule —— 平台→独立流程微服务的**反向代理壳**（S6 center_client 对接）。
//!
//! 「前端一芯三壳」在后端的对偶：`FlowModule`（进程内嵌引擎，`/api/flow/*` 由本进程 handler 处理）
//! ↔ `FlowProxyModule`（引擎在**远程独立 flow-server**，`/api/flow/*` 透明转发到它）。二者对
//! web-server 是同一个 `impl ModuleRoutes` 契约、同一段 `/flow/*` 前缀——**前端零改**（浏览器仍
//! 请求同源 `/api/flow/...`），切换只看 `[center_client.urls].flow` 配没配。
//!
//! 出站鉴权对齐平台既有 `remote_importers::apply_auth_headers` 三层：
//!   ① `X-API-Key`         —— 平台服务身份（`[service_auth].outgoing_api_key`）
//!   ② `X-Delegated-User-Token: Bearer <JWT>` —— 当前登录用户原始令牌（on-behalf-of，真实办理人）
//!   ③ `X-Request-Id`      —— 链路追踪
//! flow-server 的 S6 认证桥（cmx-flow-app auth）据此：API Key 验服务身份，委托令牌解真实用户+租户。
//!
//! 路径映射：`/flow/{rest}`（经 web-server `nest("/api")` → 实际 `/api/flow/{rest}`）转发到
//! `{flow_base}/api/flow/v1/{rest}`（**升级到 v1 正式契约**）。query 原样透传，body 双向流式。

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::any;
use axum::Router;

use cmx_api::CmxAppState;
use cmx_api::routes::traits::ModuleRoutes;

/// 反代模块：持远程 flow-server 基址 + 出站服务凭证 + 复用的 HTTP 客户端。
#[derive(Clone)]
pub struct FlowProxyModule {
    inner: std::sync::Arc<ProxyState>,
}

struct ProxyState {
    /// 远程 flow-server 基址（如 `http://flow-server:8091`），来自 `[center_client.urls].flow`。
    flow_base: String,
    /// 平台对外服务凭证（`[service_auth].outgoing_api_key`，注入 X-API-Key）。可空（不注入）。
    api_key: Option<String>,
    client: reqwest::Client,
}

impl FlowProxyModule {
    /// 用远程基址 + 出站 API Key 构建。基址末尾多余 `/` 去掉（拼接时统一补）。
    pub fn new(flow_base: impl Into<String>, api_key: Option<String>) -> Self {
        let flow_base = flow_base.into().trim_end_matches('/').to_string();
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_else(|e| {
                tracing::error!(error = %e, "FlowProxy 构建 reqwest 客户端失败，退回默认");
                reqwest::Client::new()
            });
        Self {
            inner: std::sync::Arc::new(ProxyState {
                flow_base,
                api_key,
                client,
            }),
        }
    }
}

impl ModuleRoutes for FlowProxyModule {
    fn routes(self) -> Router<CmxAppState> {
        // 捕获 /flow 与 /flow/*rest 两种（根与子路径）。自持 State=proxy，故对 CmxAppState 是
        // 一个已 with_state 的子 Router，merge 进主路由不影响主 state。
        let proxy = self.inner;
        Router::new()
            .route("/flow", any(proxy_handler))
            .route("/flow/{*rest}", any(proxy_handler))
            .with_state(proxy)
    }

    fn prefix() -> &'static str {
        "flow"
    }

    fn module_name(&self) -> &'static str {
        "flow-proxy"
    }
}

/// 转发 handler：重写 URL → 注入三层鉴权 → 流式转发请求/响应。
async fn proxy_handler(State(px): State<std::sync::Arc<ProxyState>>, req: Request) -> Response {
    let method = req.method().clone();
    let uri = req.uri().clone();
    let headers = req.headers().clone();

    // 目标 URL：/api/flow/{rest}（本进程收到的路径，nest /api 已剥到 /flow/rest）→
    // {flow_base}/api/flow/v1/{rest}。取 uri.path() 里 "/flow/" 之后的部分。
    let path = uri.path();
    let rest = path.strip_prefix("/flow/").or_else(|| path.strip_prefix("/flow")).unwrap_or("");
    let rest = rest.trim_start_matches('/');
    let query = uri.query().map(|q| format!("?{q}")).unwrap_or_default();
    let target = if rest.is_empty() {
        format!("{}/api/flow/v1{query}", px.flow_base)
    } else {
        format!("{}/api/flow/v1/{rest}{query}", px.flow_base)
    };

    // 请求体 → reqwest stream（双向流式，避免整体缓冲）。
    let body = req.into_body();
    let reqwest_body = reqwest::Body::wrap_stream(body.into_data_stream());

    let mut rb = px
        .client
        .request(method, &target)
        .body(reqwest_body);

    // 透传原请求头（除逐跳/host，避免污染）。
    rb = rb.headers(forward_request_headers(&headers));

    // 三层出站鉴权（对齐 remote_importers::apply_auth_headers）。
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
            tracing::error!(error = %e, %target, "FlowProxy 转发失败");
            (
                StatusCode::BAD_GATEWAY,
                axum::Json(serde_json::json!({
                    "code": 502,
                    "msg": format!("流程服务不可达: {e}")
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
        // 逐跳头不回传；其余（含 content-type、cache-control、ETag、text/event-stream）原样。
        let name = HeaderName::from_bytes(k.as_ref());
        let val = HeaderValue::from_bytes(v.as_bytes());
        if let (Ok(name), Ok(val)) = (name, val) {
            if is_hop_by_hop(&name) {
                continue;
            }
            headers.insert(name, val);
        }
    }
    // 流式体（SSE /events 也走这条：content-type text/event-stream 保留，逐块透传）。
    let stream = resp.bytes_stream();
    let body = Body::from_stream(stream);
    let mut out = (status, body).into_response();
    *out.headers_mut() = headers;
    out
}

/// 保留 Uri 供未来扩展（当前 target 拼接直接用 path/query）。
#[allow(dead_code)]
fn _uri_hint(_u: &Uri) {}
