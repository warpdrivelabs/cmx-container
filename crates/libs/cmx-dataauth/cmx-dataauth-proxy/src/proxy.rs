//! DataAuthProxyModule —— 平台→独立数据权限微服务的**反向代理壳**（对标 cmx-meta-proxy）。
//!
//! 「后端一芯多壳」：门户 `/api/dataauth/*` 透明转发到远程 cmx-dataauth-server。恒等映射
//! `{base}/api{原path}{query}`（引擎已暴露 `/api/dataauth/v1/*`，故无需插 `/v1`）。另经 [`console_routes`]
//! 把**顶层** `/console` 管理工作台整页反代过去（非 `/api`，故不剥前缀）。头卫生/超时/流式/三层出站鉴权/
//! 502-503 兜底全在转发核 [`cmx_proxy_core::ProxyCore`]（各反代壳共用）。

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
pub struct DataAuthProxyModule {
    inner: std::sync::Arc<ProxyCore>,
}

impl DataAuthProxyModule {
    /// 用目标 resolver + 出站 API Key 构建。
    pub fn with_resolver(resolver: UpstreamResolver, api_key: Option<String>) -> Self {
        Self {
            inner: std::sync::Arc::new(ProxyCore::new(resolver, api_key)),
        }
    }
}

impl ModuleRoutes for DataAuthProxyModule {
    fn routes(self) -> Router<CmxAppState> {
        // 数据权限单一前缀 `/dataauth`（根 + 子路径）。自持 State=proxy，merge 进主路由不影响主 state。
        let proxy = self.inner;
        Router::new()
            .route("/dataauth", any(api_handler))
            .route("/dataauth/{*rest}", any(api_handler))
            .with_state(proxy)
    }

    fn prefix() -> &'static str {
        "dataauth"
    }

    fn module_name(&self) -> &'static str {
        "dataauth-proxy"
    }
}

async fn api_handler(State(px): State<std::sync::Arc<ProxyCore>>, req: Request) -> Response {
    px.forward("数据权限服务", req, api_target).await
}

/// 恒等路径重写：本进程 path 已被外层 nest("/api") 剥去 `/api`（如 `/dataauth/v1/decide`），补回即可。
fn api_target(base: &str, uri: &Uri) -> String {
    let query = uri.query().map(|q| format!("?{q}")).unwrap_or_default();
    format!("{base}/api{}{query}", uri.path())
}

// ============================================================================
// 顶层工作台反代：`/console` 是独立 HTML 管理工作台整页（非 native/html 页描述符，不走微前端联邦）。
// 平台在 router.rs 顶层（`/api` nest 之外、与 /_mon 同层）merge 本路由，故路径不含 `/api`，恒等透传。
// ============================================================================

/// 顶层 `/console`（+ 子路径）与 `/swagger` 反代到独立 cmx-dataauth-server。免认证边缘页；
/// 其 API 调用仍走 `/api/dataauth/*` 认证。`/swagger` 一并反代，使内嵌工作台的"API 文档"链接可用
/// （openapi.json 已在 `/api/dataauth/*` 内）。
pub fn console_routes(resolver: UpstreamResolver, api_key: Option<String>) -> Router {
    let px = std::sync::Arc::new(ProxyCore::new(resolver, api_key));
    Router::new()
        .route("/console", any(console_handler))
        .route("/console/{*rest}", any(console_handler))
        .route("/swagger", any(console_handler))
        .with_state(px)
}

async fn console_handler(State(px): State<std::sync::Arc<ProxyCore>>, req: Request) -> Response {
    px.forward("数据权限工作台", req, console_target).await
}

/// 顶层恒等透传：`/console` → `{base}/console`（不补 `/api`）。
fn console_target(base: &str, uri: &Uri) -> String {
    let query = uri.query().map(|q| format!("?{q}")).unwrap_or_default();
    format!("{base}{}{query}", uri.path())
}
