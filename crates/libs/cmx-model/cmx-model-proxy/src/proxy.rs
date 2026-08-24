//! ModelProxyModule —— 平台→独立模型中心微服务的**反向代理壳**（对标 cmx-rpt-api 的 ReportProxyModule）。
//!
//! 「后端一芯双壳」：进程内嵌（`/api/dct`、`/api/doc`、`/api/model`、`/api/code` 由 cmx-container 内
//! 的 Dct/Doc/Model/Code 模块 handler 处理）↔ ModelProxyModule（引擎在**远程独立 cmx-model-server**，
//! 同前缀透明转发过去）。二者对 web-server 是同一批模型中心前缀——**前端零改**，切换只看
//! `[center_client.services].model` 是否配置。
//!
//! 与报表一致：模型中心微服务的对外 URL 与平台**完全一致**（无 `/v1` 升级），故转发路径是恒等映射
//! `{model_base}/api{原path}{query}`。
//!
//! 覆盖前缀：`/dct`、`/dict`、`/doc`、`/model`、`/definitions`、`/flexible-combination`、`/code`。
//! 壳与核分工同报表：本壳只管恒等路径映射与页面归属判定；头卫生/三层出站鉴权/超时/流式/502-503
//! 兜底全在转发核 [`cmx_proxy_core::ProxyCore`]（四反代壳共用）。

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
pub struct ModelProxyModule {
    inner: std::sync::Arc<ProxyCore>,
}

impl ModelProxyModule {
    /// 用目标 resolver + 出站 API Key 构建（API 反代与页面反代共享同一转发核/连接池）。
    pub fn with_resolver(resolver: UpstreamResolver, api_key: Option<String>) -> Self {
        Self {
            inner: std::sync::Arc::new(ProxyCore::new(resolver, api_key)),
        }
    }
}

impl ModuleRoutes for ModelProxyModule {
    fn routes(self) -> Router<CmxAppState> {
        // 覆盖模型中心七前缀（根与子路径）。自持 State=proxy，故对 CmxAppState 是一个已 with_state 的
        // 子 Router，merge 进主路由不影响主 state。
        let proxy = self.inner;
        Router::new()
            .route("/dct", any(proxy_handler))
            .route("/dct/{*rest}", any(proxy_handler))
            .route("/dict", any(proxy_handler))
            .route("/dict/{*rest}", any(proxy_handler))
            .route("/doc", any(proxy_handler))
            .route("/doc/{*rest}", any(proxy_handler))
            .route("/model", any(proxy_handler))
            .route("/model/{*rest}", any(proxy_handler))
            .route("/definitions", any(proxy_handler))
            .route("/definitions/{*rest}", any(proxy_handler))
            .route("/flexible-combination", any(proxy_handler))
            .route("/flexible-combination/{*rest}", any(proxy_handler))
            .route("/code", any(proxy_handler))
            .route("/code/{*rest}", any(proxy_handler))
            .with_state(proxy)
    }

    fn prefix() -> &'static str {
        "model"
    }

    fn module_name(&self) -> &'static str {
        "model-proxy"
    }
}

/// 转发 handler：解析目标基址后恒等映射，交转发核。
async fn proxy_handler(State(px): State<std::sync::Arc<ProxyCore>>, req: Request) -> Response {
    forward(&px, req).await
}

/// 复用的恒等转发：`{model_base}/api{path}{query}`。被 `proxy_handler`（API 反代）与
/// `page_proxy_mw`（页面反代）共用。
async fn forward(px: &ProxyCore, req: Request) -> Response {
    px.forward("模型中心服务", req, model_target).await
}

/// 模型中心路径重写（恒等）：本进程收到的 path 已被外层 nest("/api") 剥去 `/api`（如
/// `/model/db-state`），模型中心微服务用同名 URL，补回 `/api` 前缀即可。
fn model_target(model_base: &str, uri: &Uri) -> String {
    let query = uri.query().map(|q| format!("?{q}")).unwrap_or_default();
    format!("{model_base}/api{}{query}", uri.path())
}

// ============================================================================
// 页面反代（F3a）：前端页 native/html 也「一芯双壳」——门户按 `[center_client].model` 把**模型中心
// 拥有的**页面取页请求反代到独立 cmx-model-server（它自暴同款字节对齐 API）。
// ----------------------------------------------------------------------------
// 与 API 反代的差异：native/html-pages 是**共享端点**（/api/native-pages/{id}），只有**部分 id**
// 属模型中心，其余是门户自己的页或 MDM 页（MDM 仍进程内嵌，另案抽独立 cmx-mdm）。故不能整前缀反代，
// 改用**中间件按 id 归属判定**：命中模型中心拥有的 id → 转发；未命中 → next.run 落回门户内嵌 handler。
// ============================================================================

/// 判定一个前端页 id 是否属模型中心（与 cmx-model/web 的清单一致）：
///   native：`portal.definition.*`、`definition.*`、`portal.dct.*`、`portal.doc.*`、
///           `portal.datasource.cluster`、`portal.dam.*`、`demo.dict-base.*`、`demo.doc-base.*`
///   html  ：`fi.cmxfico.gl.dict*`（三套工作台 dictflat/dicttree/dictcls + dictrel/dictws +
///           dict-editor-demo/dict-grid-meta 等）、`fi.cmxfico.gl.dct-*`、`fi.cmxfico.gl.meta-model-*`
/// **不含** `portal.mdm.*`（MDM 仍内嵌，另案抽 cmx-mdm）与门户自有页（job/help/notify/system 等）。
fn is_model_owned_page(id: &str) -> bool {
    // native
    id.starts_with("portal.definition.")
        || id.starts_with("definition.")
        || id.starts_with("portal.dct.")
        || id.starts_with("portal.doc.")
        || id == "portal.datasource.cluster"
        || id.starts_with("portal.dam.")
        || id.starts_with("demo.dict-base.")
        || id.starts_with("demo.doc-base.")
        // html（模型中心拥有的字典/单据定义工作台）
        || id.starts_with("fi.cmxfico.gl.dict")
        || id.starts_with("fi.cmxfico.gl.dct-")
        || id.starts_with("fi.cmxfico.gl.meta-model-")
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

/// 页面反代中间件：模型中心拥有的单页取页请求 → 转发 cmx-model-server；其余 → 落回门户 handler。
/// 由平台在配置了反代目标时对 api 路由 `layer` 之。
async fn page_proxy_mw(
    State(px): State<std::sync::Arc<ProxyCore>>,
    req: Request,
    next: axum::middleware::Next,
) -> Response {
    if let Some(id) = page_id_of(req.uri().path())
        && is_model_owned_page(id)
    {
        return forward(&px, req).await;
    }
    next.run(req).await
}

/// 给 api 路由叠加**模型中心页面反代**层：模型中心拥有的 native/html 单页取页请求转发到独立
/// cmx-model-server，其余落回门户内嵌 handler。复用 `ModelProxyModule` 的目标 resolver + 出站凭证
/// + HTTP 客户端（与 API 反代同一连接池）。平台 `merge_model` 在配置了反代目标时调它。
pub fn with_model_page_proxy(
    router: Router<CmxAppState>,
    resolver: UpstreamResolver,
    api_key: Option<String>,
) -> Router<CmxAppState> {
    let state = ModelProxyModule::with_resolver(resolver, api_key).inner;
    router.layer(axum::middleware::from_fn_with_state(state, page_proxy_mw))
}
