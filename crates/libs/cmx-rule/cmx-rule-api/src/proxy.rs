//! RulesProxyModule —— 平台→独立决策规则微服务的**反向代理壳**（对标 cmx-rpt-api 的 ReportProxyModule）。
//!
//! 规则引擎无进程内嵌壳（始终独立微服务）：`[center_client]` 的服务定位配置了 `rules` 键 →
//! 挂本反代，平台 `/api/rules/*` 透明转发到远程 cmx-rule-server；没配 → 平台无规则路由
//! （规则页无法加载）。per-key 定位：`services.rules` 配 url 静态基址或 discovery Nacos 选例
//!（见 `cmx_plugin::center_client::upstream::proxy_upstream`）。
//!
//! 规则微服务对外 URL 与平台一致（`/api/rules/v1/*`，无路径重写），故转发是恒等映射
//! `{rules_base}/api{原path}{query}`（与 report 同，不重写路径段）。
//!
//! 目标经 [`UpstreamResolver`] 按请求动态解析（静态基址 / Nacos 服务发现选例），
//! 无可用实例 → 503（区别于下游不可达的 502）。
//!
//! 壳与核的分工：本壳只管恒等路径映射与页面归属判定；头卫生（P0：剥客户端可伪造的注入型
//! 头/Cookie）、三层出站鉴权（`X-API-Key` / `X-Delegated-User-Token: Bearer` / `X-Request-Id`）、
//! 超时语义（connect/read 拆分，不设总超时保 SSE 长流）、流式转发、502/503 兜底全在转发核
//! [`cmx_proxy_core::ProxyCore`]（三反代壳共用）。

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
pub struct RulesProxyModule {
    inner: std::sync::Arc<ProxyCore>,
}

impl RulesProxyModule {
    /// 用目标 resolver + 出站 API Key 构建（API 反代与页面反代共享同一转发核/连接池）。
    pub fn with_resolver(resolver: UpstreamResolver, api_key: Option<String>) -> Self {
        Self {
            inner: std::sync::Arc::new(ProxyCore::new(resolver, api_key)),
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

/// 转发 handler：解析目标基址后恒等映射，交转发核。
async fn proxy_handler(State(px): State<std::sync::Arc<ProxyCore>>, req: Request) -> Response {
    forward(&px, req).await
}

/// 复用的恒等转发：`{rules_base}/api{path}{query}`。被 `proxy_handler`（API 反代）与
/// `page_proxy_mw`（页面反代）共用。
async fn forward(px: &ProxyCore, req: Request) -> Response {
    px.forward("规则服务", req, rules_target).await
}

/// 规则路径重写（恒等）：本进程收到的 path 已被外层 nest("/api") 剥去 `/api`（如
/// `/rules/v1/definitions`），规则微服务用同名 URL，补回 `/api` 前缀即可。
fn rules_target(rules_base: &str, uri: &Uri) -> String {
    let query = uri.query().map(|q| format!("?{q}")).unwrap_or_default();
    format!("{rules_base}/api{}{query}", uri.path())
}

// ============================================================================
// 页面反代：native 页也「一芯双壳」——门户按 [center_client] 的服务定位配置把**规则拥有的**页面取页
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
    State(px): State<std::sync::Arc<ProxyCore>>,
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
/// 其余落回门户内嵌 handler。复用 `RulesProxyModule` 的目标 resolver + 出站凭证 + HTTP 客户端
/// （与 API 反代同一连接池）。平台 `merge_rules` 在配置了反代目标时调它。
pub fn with_rules_page_proxy(
    router: Router<CmxAppState>,
    resolver: UpstreamResolver,
    api_key: Option<String>,
) -> Router<CmxAppState> {
    let state = RulesProxyModule::with_resolver(resolver, api_key).inner;
    router.layer(axum::middleware::from_fn_with_state(state, page_proxy_mw))
}
