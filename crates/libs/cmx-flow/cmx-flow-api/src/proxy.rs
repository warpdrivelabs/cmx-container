//! FlowProxyModule —— 平台→独立流程微服务的**反向代理壳**（S6 center_client 对接）。
//!
//! 「前端一芯三壳」在后端的对偶：`FlowModule`（进程内嵌引擎，`/api/flow/*` 由本进程 handler 处理）
//! ↔ `FlowProxyModule`（引擎在**远程独立 flow-server**，`/api/flow/*` 透明转发到它）。二者对
//! web-server 是同一个 `impl ModuleRoutes` 契约、同一段 `/flow/*` 前缀——**前端零改**（浏览器仍
//! 请求同源 `/api/flow/...`），切换只看 `[center_client]` 的服务定位配置（per-key：`services.flow`
//! 配 url 静态基址或 discovery Nacos 选例，见
//! `cmx_plugin::center_client::upstream::proxy_upstream`）。
//!
//! 目标经 [`UpstreamResolver`] 按请求动态解析：静态基址固化返回；Nacos 服务发现模式每次从
//! 全局实例缓存选例（订阅推送 + 30s 同步保新鲜）。无可用实例 → 503（区别于下游不可达的 502）。
//!
//! 壳与核的分工：本壳只管**路径重写**（`/flow/{rest}` → `{flow_base}/api/flow/v1/{rest}`，升级到
//! v1 正式契约）与**页面归属判定**；头卫生（P0：剥客户端可伪造的注入型头/Cookie）、三层出站
//! 鉴权、超时语义（connect/read 拆分，不设总超时保 SSE 长流）、流式转发、502/503 兜底全在
//! 转发核 [`cmx_proxy_core::ProxyCore`]（三反代壳共用，一处定义一处修复）。
//!
//! 出站鉴权对齐平台既有 `remote_importers::apply_auth_headers` 三层：
//!   ① `X-API-Key`         —— 平台服务身份（`[service_auth].outgoing_api_key`）
//!   ② `X-Delegated-User-Token: Bearer <JWT>` —— 当前登录用户原始令牌（on-behalf-of，真实办理人）
//!   ③ `X-Request-Id`      —— 链路追踪
//! flow-server 的 S6 认证桥（cmx-flow-app auth）据此：API Key 验服务身份，委托令牌解真实用户+租户。

use axum::extract::{Request, State};
use axum::http::Uri;
use axum::response::Response;
use axum::routing::any;
use axum::Router;

use cmx_api_core::CmxAppState;
use cmx_api_core::routes::traits::ModuleRoutes;
use cmx_proxy_core::ProxyCore;

pub use cmx_proxy_core::UpstreamResolver;

/// 反代模块：持转发核（目标 resolver + 出站凭证 + 连接池）。
#[derive(Clone)]
pub struct FlowProxyModule {
    inner: std::sync::Arc<ProxyCore>,
}

impl FlowProxyModule {
    /// 用目标 resolver + 出站 API Key 构建（API 反代与页面反代共享同一转发核/连接池）。
    pub fn with_resolver(resolver: UpstreamResolver, api_key: Option<String>) -> Self {
        Self {
            inner: std::sync::Arc::new(ProxyCore::new(resolver, api_key)),
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

/// 转发 handler：动态解析目标基址，经 flow 路径重写（升 v1）后交转发核。
async fn proxy_handler(State(px): State<std::sync::Arc<ProxyCore>>, req: Request) -> Response {
    px.forward("流程服务", req, flow_target).await
}

/// flow 路径重写：`/flow/{rest}`（经 web-server `nest("/api")` → 实际 `/api/flow/{rest}`）→
/// `{flow_base}/api/flow/v1/{rest}`（**升级到 v1 正式契约**）。query 原样透传。
fn flow_target(flow_base: &str, uri: &Uri) -> String {
    let path = uri.path();
    let rest = path
        .strip_prefix("/flow/")
        .or_else(|| path.strip_prefix("/flow"))
        .unwrap_or("");
    let rest = rest.trim_start_matches('/');
    let query = uri.query().map(|q| format!("?{q}")).unwrap_or_default();
    if rest.is_empty() {
        format!("{flow_base}/api/flow/v1{query}")
    } else {
        format!("{flow_base}/api/flow/v1/{rest}{query}")
    }
}

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

/// 页面反代转发：恒等转发到 `{flow_base}/api{path}{query}`（页面不升 /v1），交转发核。
async fn forward_page(px: &ProxyCore, req: Request) -> Response {
    px.forward("流程服务", req, |flow_base, uri| {
        let query = uri.query().map(|q| format!("?{q}")).unwrap_or_default();
        format!("{flow_base}/api{}{query}", uri.path())
    })
    .await
}

/// 页面反代中间件：流程拥有的单页取页请求 → 转发 flow-server；其余 → 落回门户 handler。
async fn page_proxy_mw(
    State(px): State<std::sync::Arc<ProxyCore>>,
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
