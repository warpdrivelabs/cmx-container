//! MdmProxyModule —— 平台→独立主数据微服务的**反向代理壳**（对标 [`cmx_rpt_api`] 的 ReportProxyModule）。
//!
//! 「后端一芯双壳」：`MdmModule`（进程内嵌，`/api/mdm/*` 由本进程 handler 处理——**已随抽取
//! 下线**）↔ `MdmProxyModule`（治理引擎在**远程独立 cmx-mdm-server**，`/api/mdm/*` 透明转发到它）。
//! 二者对 web-server 是同一个 `impl ModuleRoutes` 契约、同一批主数据前缀——**前端零改**（浏览器
//! 仍请求同源 `/api/mdm/...`），切换只看 `[center_client]` 的服务定位配置（per-key：
//! `services.mdm` 配 url 静态基址或 discovery Nacos 选例，见
//! `cmx_plugin::center_client::upstream::proxy_upstream`）。
//!
//! 与 report 同形：主数据微服务的对外 URL 与平台**完全一致**（`/api/mdm/*`），故转发路径是恒等
//! 映射 `{mdm_base}/api{原path}{query}`，不重写路径段。
//!
//! 目标经 [`UpstreamResolver`] 按请求动态解析（静态基址 / Nacos 服务发现选例），
//! 无可用实例 → 503（区别于下游不可达的 502）。
//!
//! 壳与核的分工：本壳只管恒等路径映射与页面归属判定；头卫生（P0：剥客户端可伪造的注入型
//! 头/Cookie）、三层出站鉴权（`X-API-Key` / `X-Delegated-User-Token: Bearer` / `X-Request-Id`）、
//! 超时语义（connect/read 拆分，不设总超时保 SSE 长流）、流式转发、502/503 兜底全在转发核
//! [`cmx_proxy_core::ProxyCore`]（反代壳共用）。

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
pub struct MdmProxyModule {
    inner: std::sync::Arc<ProxyCore>,
}

impl MdmProxyModule {
    /// 用目标 resolver + 出站 API Key 构建（API 反代与页面反代共享同一转发核/连接池）。
    pub fn with_resolver(resolver: UpstreamResolver, api_key: Option<String>) -> Self {
        Self {
            inner: std::sync::Arc::new(ProxyCore::new(resolver, api_key)),
        }
    }
}

impl ModuleRoutes for MdmProxyModule {
    fn routes(self) -> Router<CmxAppState> {
        // 覆盖主数据前缀（根与子路径）。自持 State=proxy，故对 CmxAppState 是一个已 with_state 的
        // 子 Router，merge 进主路由不影响主 state。
        let proxy = self.inner;
        Router::new()
            .route("/mdm", any(proxy_handler))
            .route("/mdm/{*rest}", any(proxy_handler))
            .with_state(proxy)
    }

    fn prefix() -> &'static str {
        "mdm"
    }

    fn module_name(&self) -> &'static str {
        "mdm-proxy"
    }
}

/// 转发 handler：解析目标基址后恒等映射，交转发核。
async fn proxy_handler(State(px): State<std::sync::Arc<ProxyCore>>, req: Request) -> Response {
    forward(&px, req).await
}

/// 复用的恒等转发：`{mdm_base}/api{path}{query}`。被 `proxy_handler`（API 反代）与
/// `page_proxy_mw`（页面反代）共用。
async fn forward(px: &ProxyCore, req: Request) -> Response {
    px.forward("主数据服务", req, mdm_target).await
}

/// 主数据路径重写（恒等）：本进程收到的 path 已被外层 nest("/api") 剥去 `/api`（如
/// `/mdm/change-requests`），主数据微服务用同名 URL，补回 `/api` 前缀即可。
fn mdm_target(mdm_base: &str, uri: &Uri) -> String {
    let query = uri.query().map(|q| format!("?{q}")).unwrap_or_default();
    format!("{mdm_base}/api{}{query}", uri.path())
}

// ============================================================================
// 页面反代：前端页 native 也「一芯双壳」——门户按 [center_client] 的服务定位配置把
// **主数据拥有的**页面取页请求反代到独立 cmx-mdm-server（它自暴同款字节对齐 API）。
// ----------------------------------------------------------------------------
// 与 API 反代的差异：native-pages 是**共享端点**（/api/native-pages/{id}），只有**部分 id**
// 属主数据，其余是门户自己的页。故不能整前缀反代，改用**中间件按 id 归属判定**：
//   - 命中主数据拥有的 id → 转发到 mdm-server；
//   - 未命中           → next.run(req) 落回门户自己的 handler（内嵌页零改）。
// 这样门户菜单打开主数据页 → 浏览器 GET /api/native-pages/portal.mdm.steward → 命中反代 →
// mdm-server 返回逐字节一致的页面源（rev 一致，ETag/缓存不错位），shell 零感知。
// ============================================================================

/// 判定一个前端页 id 是否属主数据（与 cmx-mdm/web 的清单一致）：native：`portal.mdm.*`。
/// 主数据无 html 页（全部为 native 页）。
fn is_mdm_owned_page(id: &str) -> bool {
    id.starts_with("portal.mdm.")
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

/// 页面反代中间件：主数据拥有的单页取页请求 → 转发 mdm-server；其余 → 落回门户 handler。
/// 由平台在配置了反代目标时对 api 路由 `layer` 之。
async fn page_proxy_mw(
    State(px): State<std::sync::Arc<ProxyCore>>,
    req: Request,
    next: axum::middleware::Next,
) -> Response {
    if let Some(id) = page_id_of(req.uri().path())
        && is_mdm_owned_page(id)
    {
        return forward(&px, req).await;
    }
    next.run(req).await
}

/// 给 api 路由叠加**主数据页面反代**层：主数据拥有的 native 单页取页请求转发到独立 cmx-mdm-server，
/// 其余落回门户内嵌 handler。复用 `MdmProxyModule` 的目标 resolver + 出站凭证 + HTTP 客户端
/// （与 API 反代同一连接池）。平台 `merge_mdm` 在配置了反代目标时调它。
pub fn with_mdm_page_proxy(
    router: Router<CmxAppState>,
    resolver: UpstreamResolver,
    api_key: Option<String>,
) -> Router<CmxAppState> {
    let state = MdmProxyModule::with_resolver(resolver, api_key).inner;
    router.layer(axum::middleware::from_fn_with_state(state, page_proxy_mw))
}
