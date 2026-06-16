//! Web 服务器主模块
//!
//! 该模块是应用程序的入口点，负责初始化各种组件、配置路由并启动 HTTP 服务器。

mod config;
mod error;
mod format;
mod routes;

pub use self::error::{Error, Result};
use config::web_config;

use axum::{middleware, Router};
use axum::extract::DefaultBodyLimit;
use crate::config::{
    init_auth_service, init_cache, init_datasources, init_infra, init_plugins, init_rpc,
    init_runtime, init_services, init_service_invoker, init_storage, shutdown_infra,
};
use cmx_api::middleware::{cors_layer, mw_auth, mw_context_resolver, trace_layer};
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

    let web_config = web_config();

    cmx_debug::init();
    info!("调试会话管理器初始化完成");

    init_runtime().await?;

    cmx_traits::GlobalEventBus::initialize()
        .map_err(|e| Error::ServerSetup(format!("初始化全局事件总线失败: {}", e)))?;
    info!("全局事件总线初始化完成");

    init_services().await?;
    init_plugins().await?;
    init_service_invoker().await?;

    // 初始化认证服务
    let auth_service = init_auth_service().await?;

    // 初始化 RPC 子系统（默认关闭，需配置 [rpc] enabled = true 启用）。
    let grpc_port = init_rpc(
        cmx_traits::GlobalServiceInvoker::get().clone(),
        cmx_runtime::GlobalExtismEngine::get_as_invoker(),
        cmx_plugin::GlobalPluginManager::get_as_plugin_query(),
    ).await?;

    // 构建完整的 AppState，注入各子系统的 trait 实例
    let app_state = CmxAppState::new()
        .with_plugin_query(cmx_plugin::GlobalPluginManager::get_as_plugin_query())
        .with_runtime_invoker(cmx_runtime::GlobalExtismEngine::get_as_invoker())
        .with_service_query(GlobalServiceQuery::get().clone())
        .with_service_storage(GlobalServiceStorage::get().clone())
        .with_storage_service(cmx_storage::global::GlobalStorageService::get().service().clone())
        .with_auth_service(auth_service);

    let api_routes = routes::routes().with_state(app_state);

    // 构建路由树，中间件顺序（从外到内）：
    // 1. CookieManager - 处理 cookies
    // 2. mw_context_resolver - 解析请求上下文
    // 3. mw_auth - 认证（Token 校验 + AuthContext 注入）
    // 4. mw_trace - 请求追踪
    // 5. RequestBodyLimitLayer - 请求体大小限制（100MB）
    // 6. cors_layer - 跨域支持
    let routes_all = Router::new()
        .nest("/api", api_routes)
        .merge(routes::get_swagger_routes())
        .layer(CookieManagerLayer::new())
        .layer(middleware::from_fn(mw_context_resolver))
        .layer(middleware::from_fn(mw_auth))
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

    axum::serve(listener, routes_all.into_make_service())
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(|e| Error::ServerSetup(format!("服务器运行失败: {}", e)))?;

    info!("开始优雅关闭...");
    shutdown_infra().await;
    info!("服务已优雅关闭");

    Ok(())
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
