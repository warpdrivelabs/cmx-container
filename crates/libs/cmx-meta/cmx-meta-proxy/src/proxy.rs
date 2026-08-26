//! MetaProxyModule —— 平台→独立元数据管理微服务的**反向代理壳**（对标 cmx-model-proxy）。
//!
//! 「后端一芯双壳」：门户 `/api/meta/*` 透明转发到远程 cmx-meta-server（:8096）。恒等映射
//! `{meta_base}/api{原path}{query}`。壳只管路径映射与页面归属判定；头卫生/超时/流式/三层出站鉴权/
//! 502-503 兜底全在转发核 [`cmx_proxy_core::ProxyCore`]（各反代壳共用）。

use axum::Router;
use axum::extract::{Request, State};
use axum::http::Uri;
use axum::response::Response;
use axum::routing::any;

use cmx_api_core::CmxAppState;
use cmx_api_core::routes::traits::ModuleRoutes;
use cmx_proxy_core::ProxyCore;

pub use cmx_proxy_core::UpstreamResolver;

/// 反代模块：持转发核（目标 resolver + 出站凭证 + 连接池）。
#[derive(Clone)]
pub struct MetaProxyModule {
    inner: std::sync::Arc<ProxyCore>,
}

impl MetaProxyModule {
    /// 用目标 resolver + 出站 API Key 构建（API 反代与页面反代共享同一转发核/连接池）。
    pub fn with_resolver(resolver: UpstreamResolver, api_key: Option<String>) -> Self {
        Self { inner: std::sync::Arc::new(ProxyCore::new(resolver, api_key)) }
    }
}

impl ModuleRoutes for MetaProxyModule {
    fn routes(self) -> Router<CmxAppState> {
        // 元数据管理单一前缀 `/meta`（根 + 子路径）。自持 State=proxy，merge 进主路由不影响主 state。
        let proxy = self.inner;
        Router::new()
            .route("/meta", any(proxy_handler))
            .route("/meta/{*rest}", any(proxy_handler))
            .with_state(proxy)
    }

    fn prefix() -> &'static str {
        "meta"
    }

    fn module_name(&self) -> &'static str {
        "meta-proxy"
    }
}

/// 转发 handler：解析目标基址后恒等映射，交转发核。
async fn proxy_handler(State(px): State<std::sync::Arc<ProxyCore>>, req: Request) -> Response {
    forward(&px, req).await
}

/// 恒等转发：`{meta_base}/api{path}{query}`。API 反代与页面反代共用。
async fn forward(px: &ProxyCore, req: Request) -> Response {
    px.forward("元数据管理服务", req, meta_target).await
}

/// 恒等路径重写：本进程 path 已被外层 nest("/api") 剥去 `/api`（如 `/meta/db-state`），补回即可。
fn meta_target(meta_base: &str, uri: &Uri) -> String {
    let query = uri.query().map(|q| format!("?{q}")).unwrap_or_default();
    format!("{meta_base}/api{}{query}", uri.path())
}

// ============================================================================
// 页面反代（F3a）：元数据管理拥有的 `meta.*` native/html 页取页请求 → 转发独立 cmx-meta-server。
// native/html-pages 是共享端点，只有 `meta.*` id 属本服务；其余落回门户内嵌 handler。
// ============================================================================

/// 判定一个前端页 id 是否属元数据管理（本服务页 id 命名空间 `meta.*`）。
fn is_meta_owned_page(id: &str) -> bool {
    id.starts_with("meta.")
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

/// 页面反代中间件：`meta.*` 单页取页请求 → 转发 cmx-meta-server；其余 → 落回门户 handler。
async fn page_proxy_mw(
    State(px): State<std::sync::Arc<ProxyCore>>,
    req: Request,
    next: axum::middleware::Next,
) -> Response {
    if let Some(id) = page_id_of(req.uri().path())
        && is_meta_owned_page(id)
    {
        return forward(&px, req).await;
    }
    next.run(req).await
}

/// 给 api 路由叠加**元数据管理页面反代**层：`meta.*` native/html 单页取页请求转发到独立
/// cmx-meta-server，其余落回门户内嵌 handler。平台 `merge_meta` 在配置了反代目标时调它。
pub fn with_meta_page_proxy(
    router: Router<CmxAppState>,
    resolver: UpstreamResolver,
    api_key: Option<String>,
) -> Router<CmxAppState> {
    let state = MetaProxyModule::with_resolver(resolver, api_key).inner;
    router.layer(axum::middleware::from_fn_with_state(state, page_proxy_mw))
}
