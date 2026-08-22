//! FlowProxyModule —— 平台→独立流程微服务的**反向代理壳**（S6 center_client 对接）。
//!
//! 「前端一芯三壳」在后端的对偶：`FlowModule`（进程内嵌引擎，`/api/flow/*` 由本进程 handler 处理）
//! ↔ `FlowProxyModule`（引擎在**远程独立 flow-server**，`/api/flow/*` 透明转发到它）。二者对
//! web-server 是同一个 `impl ModuleRoutes` 契约、同一段 `/flow/*` 前缀——**前端零改**（浏览器仍
//! 请求同源 `/api/flow/...`），切换只看 `[center_client]` 的服务定位配置（mode 驱动：http_url 模式
//! 看 `urls.flow`，http_discovery/grpc 模式看 `discovery.services.flow`，见
//! `cmx_plugin::center_client::upstream::proxy_upstream`）。
//!
//! 目标经 [`UpstreamResolver`] 按请求动态解析：静态基址固化返回；Nacos 服务发现模式每次从
//! 全局实例缓存选例（订阅推送 + 30s 同步保新鲜）。无可用实例 → 503（区别于下游不可达的 502）。
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

use cmx_api_core::CmxAppState;
use cmx_api_core::routes::traits::ModuleRoutes;

/// 反代目标 resolver：每次调用返回当前可用基址（`None` = 无可用实例 → 503）。
///
/// 由装配层（cmx-platform-app routes.rs）从 `cmx_plugin::center_client::ProxyUpstream::resolver_fn`
/// 构造——`Send + Sync` 无状态闭包，`Static` 固化返回基址，`Discovery` 查内存实例缓存。
pub type UpstreamResolver = std::sync::Arc<dyn Fn() -> Option<String> + Send + Sync>;

/// 反代模块：持目标 resolver + 出站服务凭证 + 复用的 HTTP 客户端。
#[derive(Clone)]
pub struct FlowProxyModule {
    inner: std::sync::Arc<ProxyState>,
}

struct ProxyState {
    /// 目标基址 resolver（静态基址或 Nacos 服务发现）。
    resolver: UpstreamResolver,
    /// 平台对外服务凭证（`[service_auth].outgoing_api_key`，注入 X-API-Key）。可空（不注入）。
    api_key: Option<String>,
    client: reqwest::Client,
}

impl FlowProxyModule {
    /// 用目标 resolver + 出站 API Key 构建（API 反代与页面反代共享同一连接池）。
    pub fn with_resolver(resolver: UpstreamResolver, api_key: Option<String>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_else(|e| {
                tracing::error!(error = %e, "FlowProxy 构建 reqwest 客户端失败，退回默认");
                reqwest::Client::new()
            });
        Self {
            inner: std::sync::Arc::new(ProxyState {
                resolver,
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

/// 转发 handler：解析目标基址 → 重写 URL → 注入三层鉴权 → 流式转发请求/响应。
async fn proxy_handler(State(px): State<std::sync::Arc<ProxyState>>, req: Request) -> Response {
    // 目标基址按请求动态解析（服务发现模式实例列表可能变化）。
    let Some(flow_base) = (px.resolver)() else {
        return no_upstream("流程服务");
    };
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
        format!("{flow_base}/api/flow/v1{query}")
    } else {
        format!("{flow_base}/api/flow/v1/{rest}{query}")
    };

    forward(&px, method, headers, target, req).await
}

/// 页面反代转发核：恒等转发到 `{flow_base}/api{path}{query}`（页面不升 /v1）。
async fn forward_page(px: &ProxyState, req: Request) -> Response {
    let Some(flow_base) = (px.resolver)() else {
        return no_upstream("流程服务");
    };
    let method = req.method().clone();
    let uri = req.uri().clone();
    let headers = req.headers().clone();
    let path = uri.path();
    let query = uri.query().map(|q| format!("?{q}")).unwrap_or_default();
    let target = format!("{flow_base}/api{path}{query}");
    forward(px, method, headers, target, req).await
}

/// 公共转发核：拼好目标 URL 后流式转发 + 三层出站鉴权（API 与页面反代共用）。
async fn forward(
    px: &ProxyState,
    method: axum::http::Method,
    headers: HeaderMap,
    target: String,
    req: Request,
) -> Response {
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

/// 目标无可用实例时的 503 响应（区别于 502 不可达：服务发现未就绪或实例全部下线）。
fn no_upstream(svc: &str) -> Response {
    tracing::error!(service = svc, "反代目标无可用实例（服务发现未就绪或实例全部下线）");
    (
        StatusCode::SERVICE_UNAVAILABLE,
        axum::Json(serde_json::json!({
            "code": 503,
            "msg": format!("{svc}无可用实例（服务发现未就绪或实例全部下线）")
        })),
    )
        .into_response()
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

// ============================================================================
// 页面反代（F3a）：前端页 native/html 也「一芯双壳」——门户按 [center_client] 的服务定位配置
// 把**流程拥有的**页面取页请求反代到独立 cmx-flow-server（它自暴同款字节对齐 API）。
// ----------------------------------------------------------------------------
// 与 flow API 反代的差异：页面**不升级 /v1**（flow-server 页面挂在 `/api/native-pages`、
// `/api/html-pages`，不在 `/api/flow/v1` 下），故用**恒等**转发 `{flow_base}/api{path}{query}`。
// 且 native/html-pages 是共享端点、仅部分 id 属流程，故按 id 归属判定：命中→转发，未命中→
// next.run 落回门户内嵌 handler。
// ============================================================================

/// 判定一个前端页 id 是否属流程（与 cmx-flowengine/web 的清单一致）：
///   native：`portal.flow.*`
///   html  ：`fi.cmxfico.gl.flow-pay-review-form`（及未来 flow-* 表单）
fn is_flow_owned_page(id: &str) -> bool {
    id.starts_with("portal.flow.") || id.starts_with("fi.cmxfico.gl.flow-")
}

/// 从 path 提取页面 id：`/native-pages/{id}` 或 `/html-pages/{id}`（已剥 `/api`）。
/// batch/list 不在此拦截（含混合 id，留门户聚合）。
fn page_id_of(path: &str) -> Option<&str> {
    for pfx in ["/native-pages/", "/html-pages/"] {
        if let Some(rest) = path.strip_prefix(pfx)
            && !rest.is_empty()
            && rest != "batch"
            && !rest.contains('/')
        {
            return Some(rest);
        }
    }
    None
}

/// 页面反代中间件：流程拥有的单页取页请求 → 转发 flow-server；其余 → 落回门户 handler。
async fn page_proxy_mw(
    State(px): State<std::sync::Arc<ProxyState>>,
    req: Request,
    next: axum::middleware::Next,
) -> Response {
    if let Some(id) = page_id_of(req.uri().path())
        && is_flow_owned_page(id)
    {
        return forward_page(&px, req).await;
    }
    next.run(req).await
}

/// 给 api 路由叠加**流程页面反代**层：流程拥有的 native/html 单页取页请求转发到独立
/// cmx-flow-server，其余落回门户内嵌 handler。复用 `FlowProxyModule` 的目标 resolver +
/// 出站凭证 + HTTP 客户端（与 API 反代同一连接池）。平台 `merge_flow` 在配置了反代目标时调它。
pub fn with_flow_page_proxy(
    router: Router<CmxAppState>,
    resolver: UpstreamResolver,
    api_key: Option<String>,
) -> Router<CmxAppState> {
    let state = FlowProxyModule::with_resolver(resolver, api_key).inner;
    router.layer(axum::middleware::from_fn_with_state(state, page_proxy_mw))
}
