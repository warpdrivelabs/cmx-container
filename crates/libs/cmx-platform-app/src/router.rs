//! 路由树组装
//!
//! 把 API 路由、中间件栈、静态文件托管组装成最终的 `Router`。中间件顺序与静态托管语义
//! 与抽取前 `main` 内联逻辑逐行一致。

use axum::extract::DefaultBodyLimit;
use axum::{Router, middleware};
use cmx_common_api::CmxAppState;
use cmx_common_api::middleware::{cors_layer, mw_auth, mw_context_resolver, mw_permission, trace_layer};
use cmx_utils::ConfigManager;
use tower_cookies::CookieManagerLayer;
use tower_http::compression::CompressionLayer;
use tower_http::compression::predicate::{DefaultPredicate, NotForContentType, Predicate};
use tower_http::limit::RequestBodyLimitLayer;
use tracing::info;

use crate::config::WebConfig;
use crate::routes;

/// 请求体大小上限（100 MiB）。
const BODY_LIMIT: usize = 100 * 1024 * 1024;

/// 组装完整路由树。
///
/// # 中间件顺序（从外到内）
/// 1. `CookieManager` - 处理 cookies
/// 2. `mw_context_resolver` - 解析请求上下文
/// 3. `mw_auth` - 认证（Token 校验 + AuthContext 注入）
/// 4. `mw_permission` - 权限校验（路由→权限码映射 + system:all 短路）
/// 5. `trace_layer` - 请求追踪
/// 6. `RequestBodyLimitLayer` - 请求体大小限制（100MB）
/// 7. `cors_layer` - 跨域支持
///
/// 之后依次挂：fallback 静态文件服务、本地存储静态路由、前端 dist 托管。
pub fn build_router(app_state: CmxAppState, web_config: &WebConfig) -> Router {
    let api_routes = routes::routes().with_state(app_state);

    let routes_all = Router::new()
        .nest("/api", api_routes)
        .merge(routes::get_swagger_routes())
        .layer(CookieManagerLayer::new())
        .layer(middleware::from_fn(mw_permission))
        // 通用技术监控遥测（夹在权限之外、认证之内层）：next.run 时 context_scope 已建（身份可读），
        // 且能采集到 403（权限拒绝）。喂 cmx-web-monitor 进程级环形缓冲，供 /_mon 技术页展示。
        .layer(middleware::from_fn(cmx_web_monitor::observe))
        .layer(middleware::from_fn(mw_auth))
        .layer(middleware::from_fn(mw_context_resolver))
        .layer(middleware::from_fn(trace_layer))
        .layer(RequestBodyLimitLayer::new(BODY_LIMIT))
        .layer(DefaultBodyLimit::max(BODY_LIMIT))
        .layer(cors_layer())
        // 响应压缩（gzip/br）：对列式 JSON 单据包压缩比高（方案 §8 四级小包）。
        // 例外 application/octet-stream：单据流式端点(/api/doc/data/tokio-zmc-stream)用
        // chunked 分帧边算边发（O(单行)内存），压缩层会缓冲整流并可能截断 gzip 尾（丢 trailer
        // → 浏览器 net::ERR_*），故排除二进制流，保持真流式。
        .layer(CompressionLayer::new().compress_when(
            DefaultPredicate::new().and(NotForContentType::const_new("application/octet-stream")),
        ));

    // 通用技术监控页（/_mon + /_mon/tech-stats）：根级、免认证、在 fallback 之前 merge，
    // 显式路由优先于 ServeDir 兜底。与 flow-server 由 chassis 自动挂的 /_mon 同一 crate、同一 UI。
    let routes_all = routes_all.merge(cmx_web_monitor::monitor_routes::<()>());

    // 添加静态文件服务作为 fallback
    let routes_all = routes_all.fallback_service(axum::routing::get_service(
        tower_http::services::ServeDir::new(&web_config.web_folder),
    ));

    let routes_all = mount_local_storage_routes(routes_all);
    mount_frontend_dists(routes_all)
}

/// 注册本地存储的静态文件访问路由（path_patterns → storage_path）。
fn mount_local_storage_routes(mut router: Router) -> Router {
    for (pattern, storage_path) in cmx_storage::global::GlobalStorageService::local_access_configs()
    {
        // 从 pattern（如 "/file/**"）提取路由前缀（如 "/file"）
        let prefix = pattern.split_once('*').map(|(p, _)| p).unwrap_or(pattern);
        let prefix = prefix.trim_end_matches('/');
        if prefix.is_empty() {
            continue;
        }
        info!("挂载本地存储静态文件路由: {} -> {}", prefix, storage_path);
        let serve_dir: Router<()> = Router::new().fallback_service(axum::routing::get_service(
            tower_http::services::ServeDir::new(storage_path),
        ));
        router = router.nest(prefix, serve_dir);
    }
    router
}

/// 同源托管两个迁移前端的生产构建（dist）+ 共享 UI5 运行时（/shared）。
///
/// `/portal` → CMXPortalManager/dist（base=/portal/），`/html` → CMXHTMLDesigner/dist
/// （base=/html/），`/shared` → cmx-ui5-runtime/dist（前端 `import("/shared/assets/...")` 动态加载）。
/// 路径由配置 `portal.web_portal_dist` / `web_html_dist` / `web_shared_dist` 给出；未配置则跳过
/// （开发走 vite 代理）。`spa=true` 未命中文件回退到 index.html（支持 history 路由）；
/// `/shared` 是纯静态（spa=false），缺文件即 404，绝不回退到某 index.html。
fn mount_frontend_dists(mut router: Router) -> Router {
    for (key, prefix, spa) in [
        ("portal.web_portal_dist", "/portal", true),
        ("portal.web_html_dist", "/html", true),
        ("portal.web_shared_dist", "/shared", false),
    ] {
        let dist = ConfigManager::global().get_string(key).unwrap_or_default();
        let dist = dist.trim();
        if dist.is_empty() {
            continue;
        }
        if !std::path::Path::new(dist).exists() {
            info!("前端 dist 未找到，跳过静态托管: {} -> {}", prefix, dist);
            continue;
        }
        info!("挂载前端静态托管: {} -> {}", prefix, dist);
        // 用 nest_service 直接挂 ServeDir（而非包一层 Router），使 /portal 与 /portal/ 都正确命中。
        let serve = if spa {
            // SPA fallback：未命中文件回退到该 dist 的 index.html，支持前端 history 路由（如 /portal/login）。
            let index = format!("{}/index.html", dist.trim_end_matches('/'));
            tower_http::services::ServeDir::new(dist)
                .fallback(tower_http::services::ServeFile::new(index))
        } else {
            tower_http::services::ServeDir::new(dist).fallback(
                // 纯静态：占位 fallback 永不命中存在的文件，缺失即由 ServeDir 返回 404。
                tower_http::services::ServeFile::new(format!(
                    "{}/__nonexistent__",
                    dist.trim_end_matches('/')
                )),
            )
        };
        router = router.nest_service(prefix, serve);
    }
    router
}
