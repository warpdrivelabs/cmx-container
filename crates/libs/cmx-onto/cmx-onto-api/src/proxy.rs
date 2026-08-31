//! OntoProxyModule —— 平台→独立本体平台微服务的**反向代理壳**（对标 cmx-rule-api 的 RulesProxyModule）。
//!
//! 本体平台无进程内嵌壳（始终独立微服务，与 rules 同构）：`[center_client]` 的服务定位配置了 `onto`
//! 键 → 挂本反代，平台 `/api/onto/*` 透明转发到远程 cmx-onto-server；没配 → 平台无本体路由
//!（本体页无法加载）。per-key 定位：`services.onto` 配 url 静态基址或 discovery Nacos 选例。
//!
//! 本体微服务对外 URL 与平台一致（`/api/onto/v1/*`，无路径重写），故转发是恒等映射
//! `{onto_base}/api{原path}{query}`（与 rules/report 同，不重写路径段）。
//!
//! 目标经 [`UpstreamResolver`] 按请求动态解析（静态基址 / Nacos 服务发现选例），无可用实例 → 503
//!（区别于下游不可达的 502）。
//!
//! 壳与核的分工：本壳只管恒等路径映射与页面归属判定；头卫生、三层出站鉴权、超时语义、流式转发、
//! 502/503 兜底全在转发核 [`cmx_proxy_core::ProxyCore`]（各反代壳共用）。

use axum::extract::{Request, State};
use axum::http::Uri;
use axum::response::Response;
use axum::routing::any;
use axum::Router;

use cmx_api_core::routes::traits::ModuleRoutes;
use cmx_api_core::CmxAppState;
use cmx_proxy_core::ProxyCore;

pub use cmx_proxy_core::UpstreamResolver;

/// 反代模块：持转发核（目标 resolver + 出站凭证 + 连接池）。
#[derive(Clone)]
pub struct OntoProxyModule {
    inner: std::sync::Arc<ProxyCore>,
}

impl OntoProxyModule {
    /// 用目标 resolver + 出站 API Key 构建（API 反代与页面反代共享同一转发核/连接池）。
    pub fn with_resolver(resolver: UpstreamResolver, api_key: Option<String>) -> Self {
        Self {
            inner: std::sync::Arc::new(ProxyCore::new(resolver, api_key)),
        }
    }
}

impl ModuleRoutes for OntoProxyModule {
    fn routes(self) -> Router<CmxAppState> {
        // 覆盖本体前缀（根与子路径）。自持 State=proxy，故对 CmxAppState 是已 with_state 的子 Router。
        let proxy = self.inner;
        Router::new()
            .route("/onto", any(proxy_handler))
            .route("/onto/{*rest}", any(proxy_handler))
            .with_state(proxy)
    }

    fn prefix() -> &'static str {
        "onto"
    }

    fn module_name(&self) -> &'static str {
        "onto-proxy"
    }
}

/// 转发 handler：解析目标基址后恒等映射，交转发核。
async fn proxy_handler(State(px): State<std::sync::Arc<ProxyCore>>, req: Request) -> Response {
    forward(&px, req).await
}

/// 复用的恒等转发：`{onto_base}/api{path}{query}`。被 `proxy_handler`（API 反代）与
/// `page_proxy_mw`（页面反代）共用。
async fn forward(px: &ProxyCore, req: Request) -> Response {
    px.forward("本体平台服务", req, onto_target).await
}

/// 本体路径重写（恒等）：本进程收到的 path 已被外层 nest("/api") 剥去 `/api`（如
/// `/onto/v1/object-types`），本体微服务用同名 URL，补回 `/api` 前缀即可。
fn onto_target(onto_base: &str, uri: &Uri) -> String {
    let query = uri.query().map(|q| format!("?{q}")).unwrap_or_default();
    format!("{onto_base}/api{}{query}", uri.path())
}

// ============================================================================
// 页面反代：native 页也「一芯双壳」——门户按 [center_client] 的服务定位配置把**本体拥有的**页面取页
// 请求反代到独立 cmx-onto-server（它自暴同款字节对齐 API）。
// ----------------------------------------------------------------------------
// native-pages 是**共享端点**（/api/native-pages/{id}），只有 `portal.onto.*` 属本体，其余是门户
// 自己的页。故用中间件按 id 归属判定：命中本体 → 转发 onto-server；未命中 → next.run 落回门户。
// ============================================================================

/// 判定一个前端页 id 是否属本体平台（与 cmx-container/assets/onto/web 的清单一致）：`portal.onto.*`。
fn is_onto_owned_page(id: &str) -> bool {
    id.starts_with("portal.onto.")
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

/// 页面反代中间件：本体拥有的单页取页请求 → 转发 onto-server；其余 → 落回门户 handler。
async fn page_proxy_mw(
    State(px): State<std::sync::Arc<ProxyCore>>,
    req: Request,
    next: axum::middleware::Next,
) -> Response {
    if let Some(id) = page_id_of(req.uri().path())
        && is_onto_owned_page(id)
    {
        return forward(&px, req).await;
    }
    next.run(req).await
}

/// 给 api 路由叠加**本体页面反代**层：本体拥有的 native 单页取页请求转发到独立 cmx-onto-server，
/// 其余落回门户内嵌 handler。复用 `OntoProxyModule` 的目标 resolver + 出站凭证 + HTTP 客户端
///（与 API 反代同一连接池）。平台 `merge_onto` 在配置了反代目标时调它。
pub fn with_onto_page_proxy(
    router: Router<CmxAppState>,
    resolver: UpstreamResolver,
    api_key: Option<String>,
) -> Router<CmxAppState> {
    let state = OntoProxyModule::with_resolver(resolver, api_key).inner;
    router.layer(axum::middleware::from_fn_with_state(state, page_proxy_mw))
}
