//! Web 服务器主模块
//!
//! 该模块是应用程序的入口点，负责初始化各种组件、配置路由并启动 HTTP 服务器。

mod config;
mod error;
mod format;
mod routes;

pub use self::error::{Error, Result};
use config::web_config;

/// 启动字符画 Logo（编译期嵌入二进制，无运行时文件路径依赖）
const BANNER: &str = include_str!("banner.txt");

use axum::{middleware, Router};
use axum::extract::DefaultBodyLimit;
use crate::config::{
    build_audit_logger, init_auth_service, init_cache, init_datasources, finalize_iam_state, init_iam_services, init_infra, init_plugins, init_rpc,
    init_runtime, init_services, init_service_invoker, init_storage, init_web_config, shutdown_infra,
};
use std::sync::Arc;
use cmx_api::middleware::{cors_layer, mw_auth, mw_context_resolver, mw_permission, trace_layer};
use cmx_api::CmxAppState;
use cmx_service::{GlobalServiceQuery, GlobalServiceStorage};
use cmx_utils::ConfigManager;
use tokio::net::TcpListener;
use tower_cookies::CookieManagerLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tracing::info;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use format::CompactFormatter;
use tracing_subscriber::{
    fmt,
    layer::SubscriberExt,
    registry,
    util::SubscriberInitExt,
    EnvFilter,
};

/// 应用程序主函数
///
/// 负责初始化配置、设置日志、初始化连接池、配置路由并启动服务器。
///
/// # 初始化顺序
///
/// 1. 日志系统（控制台 + 文件双输出）
/// 2. 全局配置（含 Nacos 远程配置覆盖）
/// 3. 加密服务
/// 4. Redis 缓存
/// 5. 数据库数据源
/// 6. 文件存储
/// 7. 调试会话
/// 8. WASM 运行时
/// 9. 全局事件总线
/// 10. 服务管理器
/// 11. 插件管理器
///
/// # Errors
///
/// * `Error::ConfigError` - 配置加载或解析失败
/// * `Error::ServerSetup` - 服务器设置失败（如地址绑定）
/// * 其他初始化错误 - 各子系统初始化失败
#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    let log_dir = "logs";
    let file_appender = RollingFileAppender::new(Rotation::DAILY, log_dir, "cmx-server.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    // 文件日志层：JSON 格式，不带 ANSI 颜色码，便于日志收集系统解析
    let file_layer = fmt::layer()
        .with_writer(non_blocking)
        .with_ansi(false)
        .with_target(true)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .with_thread_names(true)
        .with_thread_ids(true)
        .json();

    // 控制台日志层：使用自定义格式化器，优化颜色和间距
    let console_layer = fmt::layer()
        .event_format(CompactFormatter)
        .with_writer(std::io::stdout)
        .with_ansi(true);

    // // 控制台日志层：简洁格式，带颜色，便于开发调试
    // let console_layer = fmt::layer()
    //     .compact()
    //     .with_writer(std::io::stdout)
    //     .with_ansi(true)
    //     .with_target(false)
    //     .with_file(true)
    //     .with_line_number(true)
    //     .with_thread_names(true)
    //     .with_thread_ids(true)
    //     .compact();


    // 环境过滤层，读取 RUST_LOG 环境变量，默认 info 级别
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    registry()
        .with(env_filter)
        .with(console_layer)
        .with(file_layer)
        .init();

    // 保持 guard 活跃直至程序结束，确保日志写入完成
    // 注意：使用 forget 而非 drop，因为 shutdown 后仍需要 flush 日志缓冲区
    std::mem::forget(guard);

    init_infra().await?;
    cmx_utils::crypto::CryptoService::init_from_env();
    info!("加密服务初始化完成");

    init_cache().await?;
    init_datasources().await?;
    init_storage().await?;

    init_web_config()
        .map_err(|e| Error::ConfigError(format!("加载 Web 配置失败: {}", e)))?;
    let web_config = web_config()
        .map_err(|e| Error::ConfigError(format!("获取 Web 配置失败: {}", e)))?;

    cmx_debug::init();
    info!("调试会话管理器初始化完成");

    init_runtime().await?;

    cmx_traits::event_bus::GlobalEventBus::initialize()
        .map_err(|e| Error::ServerSetup(format!("初始化全局事件总线失败: {}", e)))?;
    info!("全局事件总线初始化完成");

    init_services().await?;
    init_plugins().await?;
    init_service_invoker().await?;

    // 初始化审计日志器（依赖 DatabaseManager，必须在 init_datasources 之后）
    let audit_logger = build_audit_logger().await?;

    // 初始化 IAM 基础服务（创建 UserAuthQueryImpl 供 AuthService 共享）
    // 同时产出 PluginDataImporter，供 HTTP 端点和 gRPC 服务端统一调用权限导入/清理逻辑。
    let (iam_state, user_auth_query, iam_config, plugin_data_importer) =
        init_iam_services(audit_logger.clone()).await?;

    // 初始化认证服务（使用 IAM 创建的 UserAuthQueryImpl）
    let auth_service = init_auth_service(user_auth_query, audit_logger.clone()).await?;

    // 用 auth_service 完成 IamState 的最终组装（注入 UserServiceImpl）
    let iam_state = finalize_iam_state(&iam_state, auth_service.clone(), iam_config, audit_logger).await?;

    // 权限一致性校验 + 权限列表日志(不写 DB,仅校验代码声明权限与 DB 是否一致)
    {
        let mm = cmx_database::get_default_db_manager();
        let db_id = mm.get_default_db_id().await;
        let mode = cmx_utils::ConfigManager::global()
            .get_string("iam.permission_consistency_mode")
            .unwrap_or_else(|_| "warn".to_string());
        if let Err(e) = cmx_iam::permission::run_consistency_check(mm, &db_id, &mode).await {
            return Err(Error::ServerSetup(format!("权限一致性校验失败: {e}")));
        }
        cmx_iam::permission::log_registered_permissions();
        cmx_iam::permission::warn_handler_annotation_status();
    }

    // 初始化 RPC 子系统（默认关闭，需配置 [rpc] enabled = true 启用）。
    // 将 PluginDataImporter 透传给 gRPC 服务端，启用 CmxPluginDataService。
    // 组装层构造 cmx-biz 的 BizFunctionInvoker（封装 RuntimeInvoker + PluginQuery）注入 cmx-rpc，
    // 使基础设施层 cmx-rpc 无需直接依赖业务层 cmx-biz。
    let function_invoker: Arc<dyn cmx_traits::function_invoker::FunctionInvoker> = Arc::new(
        cmx_biz::function_invoker::BizFunctionInvoker::new(
            cmx_runtime::GlobalExtismEngine::get_as_invoker(),
            cmx_plugin::GlobalPluginManager::get_as_plugin_query(),
        ),
    );
    let grpc_port = init_rpc(
        cmx_traits::service::GlobalServiceInvoker::get().clone(),
        function_invoker,
        plugin_data_importer.clone(),
    ).await?;

    // 构建完整的 AppState，注入各子系统的 trait 实例
    let app_state = CmxAppState::new()
        .with_plugin_query(cmx_plugin::GlobalPluginManager::get_as_plugin_query())
        .with_runtime_invoker(cmx_runtime::GlobalExtismEngine::get_as_invoker())
        .with_service_query(GlobalServiceQuery::get().clone())
        .with_service_storage(GlobalServiceStorage::get().clone())
        .with_storage_service(cmx_storage::global::GlobalStorageService::get().service().clone())
        .with_auth_service(auth_service)
        .with_iam(iam_state);

    // 注入 PluginDataImporter（HTTP 端点 /iam/permissions/import 和 /cleanup 使用）
    let app_state = if let Some(importer) = plugin_data_importer {
        app_state.with_plugin_data_importer(importer)
    } else {
        app_state
    };

    let api_routes = routes::routes().with_state(app_state);

    // 构建路由树，中间件顺序（从外到内）：
    // 1. CookieManager - 处理 cookies
    // 2. mw_context_resolver - 解析请求上下文
    // 3. mw_auth - 认证（Token 校验 + AuthContext 注入）
    // 4. mw_permission - 权限校验（路由→权限码映射 + system:all 短路）
    // 5. mw_trace - 请求追踪
    // 6. RequestBodyLimitLayer - 请求体大小限制（100MB）
    // 7. cors_layer - 跨域支持
    let routes_all = Router::new()
        .nest("/api", api_routes)
        .merge(routes::get_swagger_routes())
        .layer(CookieManagerLayer::new())
        .layer(middleware::from_fn(mw_permission))
        .layer(middleware::from_fn(mw_auth))
        .layer(middleware::from_fn(mw_context_resolver))
        .layer(middleware::from_fn(trace_layer))
        .layer(RequestBodyLimitLayer::new(100 * 1024 * 1024))
        .layer(DefaultBodyLimit::max(100 * 1024 * 1024))
        .layer(cors_layer());

    // 添加静态文件服务作为 fallback
    let routes_all = routes_all.fallback_service(axum::routing::get_service(
        tower_http::services::ServeDir::new(&web_config.web_folder),
    ));

    // 注册本地存储的静态文件访问路由（path_patterns → storage_path）
    let routes_all = {
        let mut router = routes_all;
        for (pattern, storage_path) in cmx_storage::global::GlobalStorageService::local_access_configs() {
            // 从 pattern（如 "/file/**"）提取路由前缀（如 "/file"）
            let prefix = pattern.split_once('*').map(|(p, _)| p).unwrap_or(pattern);
            let prefix = prefix.trim_end_matches('/');
            if prefix.is_empty() {
                continue;
            }
            info!("挂载本地存储静态文件路由: {} -> {}", prefix, storage_path);
            let serve_dir: axum::Router<()> = axum::Router::new()
                .fallback_service(axum::routing::get_service(
                    tower_http::services::ServeDir::new(storage_path),
                ));
            router = router.nest(prefix, serve_dir);
        }
        router
    };

    // 同源托管两个迁移前端的生产构建（dist）+ 共享 UI5 运行时（/shared）：
    //   /portal -> CMXPortalManager/dist（base=/portal/），/html -> CMXHTMLDesigner/dist（base=/html/），
    //   /shared -> cmx-ui5-runtime/dist（UI5/Tabler 运行时，前端用 import("/shared/assets/...") 动态加载）。
    // 路径由配置 portal.web_portal_dist / portal.web_html_dist / portal.web_shared_dist 给出；
    // 未配置则跳过（开发时走 vite 代理）。spa=true 的前端未命中文件回退到 index.html（支持 history 路由）；
    // /shared 是纯静态资源（spa=false），缺文件即 404，绝不能回退到某个 index.html。
    let routes_all = {
        let mut router = routes_all;
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
    };

    let server_host = ConfigManager::global()
        .get_string("server.host")
        .unwrap_or_else(|_| "0.0.0.0".to_string());
    let server_port: u16 = ConfigManager::global()
        .get_string("server.port")
        .unwrap_or_else(|_| "8080".to_string())
        .parse()
        .unwrap_or(8080);

    let listener = TcpListener::bind(format!("{}:{}", server_host, server_port))
        .await
        .map_err(|e| Error::ServerSetup(format!("绑定地址失败: {}", e)))?;

    let actual_port = listener.local_addr()?.port();

    info!("{}", "=".repeat(60));
    info!("🚀 {:<44} 🚀", "Web 服务器启动成功");
    info!("{}", "=".repeat(60));
    info!("   监听地址：{}:{}", server_host, actual_port);
    info!("   (配置端口：{})", server_port);
    info!("   静态文件目录：{}", web_config.web_folder);
    info!("   日志目录：{}", log_dir);
    if let Some(port) = grpc_port {
        info!("   gRPC 端口：{}", port);
    }
    info!("{}", "-".repeat(60));

    // 启动字符画 Logo（直接打印到 stdout，避免被日志格式化器加上时间戳/级别前缀，
    // 仿照 Redis 启动时的 ASCII 艺术字效果）。放在所有启动日志之后，作为最后输出。
    print_banner();

    axum::serve(listener, routes_all.into_make_service())
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(|e| Error::ServerSetup(format!("服务器运行失败: {}", e)))?;

    info!("开始优雅关闭...");
    shutdown_infra().await;
    info!("服务已优雅关闭");

    Ok(())
}

/// 打印带渐变色的启动 Logo。
///
/// 对 [`BANNER`] 逐行施加「青 → 蓝 → 紫 → 品红」的纵向 24-bit 真彩渐变。
/// 当 stdout 不是终端时（如重定向到文件 / 日志收集管道）降级为纯文本输出，
/// 避免把 ANSI 转义码写进日志。
fn print_banner() {
    use std::io::IsTerminal;

    // Logo 下方的标语
    const TAGLINE: &str = "  Enterprise Business Container V1.1.6 ";

    // 非终端：输出纯文本，避免 ANSI 码污染日志
    if !std::io::stdout().is_terminal() {
        println!("{}", BANNER);
        println!("{}", TAGLINE);
        return;
    }

    // 渐变停靠点（RGB）：青 → 蓝 → 紫 → 品红
    const STOPS: [(u8, u8, u8); 4] = [
        (0, 229, 255),   // 青
        (41, 121, 255),  // 蓝
        (124, 77, 255),  // 紫
        (255, 64, 200),  // 品红
    ];

    let lines: Vec<&str> = BANNER.lines().collect();
    // 仅按「有内容的行」计算渐变位置，空行不参与
    let total = lines.iter().filter(|l| !l.trim().is_empty()).count();
    let denom = total.saturating_sub(1).max(1) as f32;

    let mut content_idx = 0usize;
    for line in &lines {
        if line.trim().is_empty() {
            println!();
            continue;
        }
        let t = content_idx as f32 / denom;
        let (r, g, b) = gradient_color(&STOPS, t);
        // \x1b[1;38;2;R;G;Bm = 加粗 + 24-bit 前景色
        println!("\x1b[1;38;2;{};{};{}m{}\x1b[0m", r, g, b, line);
        content_idx += 1;
    }

    // 字符画之后换行，再打印标语（取渐变末端的品红色，加粗）
    let (r, g, b) = STOPS[STOPS.len() - 1];
    println!("\n\x1b[1;38;2;{};{};{}m{}\x1b[0m", r, g, b, TAGLINE);
}

/// 在多个 RGB 停靠点之间按 `t ∈ [0,1]` 线性插值，得到渐变色。
fn gradient_color(stops: &[(u8, u8, u8)], t: f32) -> (u8, u8, u8) {
    let seg = stops.len().saturating_sub(1);
    if seg == 0 {
        return stops.first().copied().unwrap_or((255, 255, 255));
    }
    let scaled = t.clamp(0.0, 1.0) * seg as f32;
    let i = (scaled.floor() as usize).min(seg - 1);
    let local = scaled - i as f32;
    let (r0, g0, b0) = stops[i];
    let (r1, g1, b1) = stops[i + 1];
    let lerp = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * local).round() as u8;
    (lerp(r0, r1), lerp(g0, g1), lerp(b0, b1))
}

/// 监听优雅关闭信号
///
/// 监听 Ctrl+C (SIGINT) 和 SIGTERM 信号，收到信号后触发 graceful shutdown。
///
/// 在 Unix 系统上同时监听两个信号，在 Windows 上只监听 Ctrl+C。
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            info!("收到 Ctrl+C 信号，开始优雅关闭...");
        },
        _ = terminate => {
            info!("收到 SIGTERM 信号，开始优雅关闭...");
        },
    }
}
