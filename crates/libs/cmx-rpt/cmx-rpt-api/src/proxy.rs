//! ReportProxyModule —— 平台→独立报表微服务的**反向代理壳**（对标 [`cmx_flow_api`] 的 FlowProxyModule）。
//!
//! 「后端一芯双壳」：`ReportModule`（进程内嵌，`/api/report-design/*` 由本进程 handler 处理）
//! ↔ `ReportProxyModule`（引擎在**远程独立 cmx-rpt-server**，`/api/report-design/*` 透明转发到它）。
//! 二者对 web-server 是同一个 `impl ModuleRoutes` 契约、同一批报表前缀——**前端零改**（浏览器仍
//! 请求同源 `/api/report-design/...`），切换只看 `[service_rpc]` 的服务定位配置（per-key：
//! `services.report` 配 url 静态基址或 discovery Nacos 选例，见
//! `cmx_service_rpc::locator`）。
//!
//! 与 flow 的差异：报表微服务的对外 URL 与平台**完全一致**（`/api/report-design/*`、
//! `/api/report-source-bindings*`、`/api/rpt/compute`、`/api/consol/*`（合并报表），
//! **无 `/v1` 升级**），故转发路径是恒等映射
//! `{report_base}/api{原path}{query}`——比 flow 更简单，不重写路径段。
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

use cmx_api_core::CmxAppState;
use cmx_api_core::routes::traits::ModuleRoutes;
use cmx_proxy_core::ProxyCore;

pub use cmx_proxy_core::UpstreamResolver;

/// 反代模块：持转发核（目标 resolver + 出站凭证 + 连接池）。
#[derive(Clone)]
pub struct ReportProxyModule {
    inner: std::sync::Arc<ProxyCore>,
}

impl ReportProxyModule {
    /// 用目标 resolver + 出站 API Key 构建（API 反代与页面反代共享同一转发核/连接池）。
    pub fn with_resolver(resolver: UpstreamResolver, api_key: Option<String>) -> Self {
        Self {
            inner: std::sync::Arc::new(ProxyCore::new(resolver, api_key)),
        }
    }
}

impl ModuleRoutes for ReportProxyModule {
    fn routes(self) -> Router<CmxAppState> {
        // 覆盖报表四前缀（根与子路径）。自持 State=proxy，故对 CmxAppState 是一个已 with_state 的
        // 子 Router，merge 进主路由不影响主 state。
        let proxy = self.inner;
        Router::new()
            .route("/report-design", any(proxy_handler))
            .route("/report-design/{*rest}", any(proxy_handler))
            .route("/report-source-bindings", any(proxy_handler))
            .route("/report-source-bindings/{*rest}", any(proxy_handler))
            .route("/rpt/{*rest}", any(proxy_handler))
            .route("/consol", any(proxy_handler))
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

/// 转发 handler：解析目标基址后恒等映射，交转发核。
async fn proxy_handler(State(px): State<std::sync::Arc<ProxyCore>>, req: Request) -> Response {
    forward(&px, req).await
}

/// 复用的恒等转发：`{report_base}/api{path}{query}`。被 `proxy_handler`（API 反代）与
/// `page_proxy_mw`（页面反代）共用。
async fn forward(px: &ProxyCore, req: Request) -> Response {
    px.forward("报表服务", req, report_target).await
}

/// 报表路径重写（恒等）：本进程收到的 path 已被外层 nest("/api") 剥去 `/api`（如
/// `/report-design/overview`），报表微服务用同名 URL，补回 `/api` 前缀即可。
fn report_target(report_base: &str, uri: &Uri) -> String {
    let query = uri.query().map(|q| format!("?{q}")).unwrap_or_default();
    format!("{report_base}/api{}{query}", uri.path())
}

// ============================================================================
// 页面反代（F3a）：前端页 native/html 也「一芯双壳」——门户按 [service_rpc] 的服务定位配置
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
///   native：`portal.rpt.*`、`portal.consol.*`（合并报表工作台）
///   html  ：`fi.cmxfico.gl.rpt-designer-*`、`fi.cmxfico.gl.rpt-spreadjs-designer-*`
///
/// 与 `cmx-common-api` 页面 handler 的属主路由表 `owner_service_of` 一一对应，新增前缀两处同步。
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

/// 页面反代中间件：报表拥有的单页取页请求 → 转发 report-server；其余 → 落回门户 handler。
/// 由平台在配置了反代目标时对 api 路由 `layer` 之。
async fn page_proxy_mw(
    State(px): State<std::sync::Arc<ProxyCore>>,
    req: Request,
    next: axum::middleware::Next,
) -> Response {
    if let Some(id) = page_id_of(req.uri().path())
        && is_report_owned_page(id)
    {
        return forward(&px, req).await;
    }
    next.run(req).await
}

/// 给 api 路由叠加**报表页面反代**层：报表拥有的 native/html 单页取页请求转发到独立 cmx-rpt-server，
/// 其余落回门户内嵌 handler。复用 `ReportProxyModule` 的目标 resolver + 出站凭证 + HTTP 客户端
/// （与 API 反代同一连接池）。平台 `merge_report` 在配置了反代目标时调它。
pub fn with_report_page_proxy(
    router: Router<CmxAppState>,
    resolver: UpstreamResolver,
    api_key: Option<String>,
) -> Router<CmxAppState> {
    let state = ReportProxyModule::with_resolver(resolver, api_key).inner;
    router.layer(axum::middleware::from_fn_with_state(state, page_proxy_mw))
}
