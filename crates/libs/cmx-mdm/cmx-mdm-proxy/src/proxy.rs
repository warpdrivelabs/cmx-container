//! MdmProxyModule —— 平台→独立主数据微服务的**反向代理壳**（对标 cmx-model-proxy 的 ModelProxyModule）。
//!
//! 「后端一芯双壳」：进程内嵌壳已随引擎抽取退役（MdmModule 源码不在 cmx-container），现存唯一
//! 形态 = MdmProxyModule（引擎在**远程独立 cmx-mdm-server**，`/api/mdm/*` 同前缀透明转发）。
//! 对 web-server 是同一 `/mdm` 前缀——**前端零改**，切换只看 `[center_client.services].mdm`。
//! 恒等映射 `{mdm_base}/api{path}{query}`。
//!
//! 壳与核分工：本壳只管恒等路径映射与页面归属判定；头卫生/三层出站鉴权/超时/流式/502-503 兜底全在
//! 转发核 [`cmx_proxy_core::ProxyCore`]（各反代壳共用）。

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

/// 恒等转发：`{mdm_base}/api{path}{query}`（本进程 path 已被外层 nest("/api") 剥去 `/api`）。
async fn forward(px: &ProxyCore, req: Request) -> Response {
    px.forward("主数据服务", req, mdm_target).await
}

fn mdm_target(mdm_base: &str, uri: &Uri) -> String {
    let query = uri.query().map(|q| format!("?{q}")).unwrap_or_default();
    format!("{mdm_base}/api{}{query}", uri.path())
}

// ============================================================================
// 页面反代（F3a）：MDM 拥有的 native 页取页请求（`portal.mdm.*`）转发到独立 cmx-mdm-server，
// 其余页请求落回门户内嵌 handler。MDM 只有 native 页（无 html）。
// ============================================================================

/// 判定一个前端页 id 是否属 MDM（与 cmx-mdm/web/ui-native 的 10 个 portal.mdm.* 一致）。
fn is_mdm_owned_page(id: &str) -> bool {
    id.starts_with("portal.mdm.")
}

/// 从 path 提取页面 id：`/native-pages/{id}` 或 `/html-pages/{id}`（已剥 `/api`）。
/// batch/list 不拦截（含混合 id，留门户聚合）。
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

/// 页面反代中间件：MDM 拥有的单页取页请求 → 转发 cmx-mdm-server；其余 → 落回门户 handler。
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

/// 给 api 路由叠加 **MDM 页面反代**层：`portal.mdm.*` 单页取页请求转发到独立 cmx-mdm-server，
/// 其余落回门户内嵌 handler。平台 `merge_mdm` 在配置了反代目标时调它。
pub fn with_mdm_page_proxy(
    router: Router<CmxAppState>,
    resolver: UpstreamResolver,
    api_key: Option<String>,
) -> Router<CmxAppState> {
    let state = MdmProxyModule::with_resolver(resolver, api_key).inner;
    router.layer(axum::middleware::from_fn_with_state(state, page_proxy_mw))
}
