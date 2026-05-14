//! Web 服务器主模块
//!
//! 该模块是应用程序的入口点，负责初始化各种组件、配置路由并启动 HTTP 服务器

mod config;
mod datasource_init;
mod error;
mod plugins;
mod routes;

pub use self::error::{Error, Result};
use config::web_config;

use axum::{middleware, Router};
use axum::extract::DefaultBodyLimit;
use crate::config::{init_cache, init_datasources, init_global_config_with_nacos, init_plugins, init_runtime, init_services, init_storage, shutdown_nacos};
use cmx_api::middleware::{cors_layer, mw_context_resolver, mw_trace};
use cmx_api::CmxAppState;
use cmx_service::{GlobalServiceQuery, GlobalServiceStorage};
use tokio::net::TcpListener;
use tower_cookies::CookieManagerLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tracing::info;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{fmt, layer::SubscriberExt, registry, util::SubscriberInitExt, EnvFilter};

/// 应用程序主函数
///
/// 负责初始化配置、设置日志、创建模型管理器、初始化连接池、配置路由并启动服务器
///
/// # 返回值
/// - `Result<()>` - 执行结果，成功返回 Ok(())，失败返回错误

#[tokio::main]
async fn main() -> Result<()> {
    // 必须先加载.env文件
    dotenvy::dotenv().ok();
    // ========== 日志文件滚动配置 ==========
    // 日志输出目录
    let log_dir = "logs";
    // 按天滚动生成新文件，可选: MINUTELY / HOURLY / DAILY / NEVER
    let file_appender = RollingFileAppender::new(Rotation::DAILY, log_dir, "cmx-server.log");
    // 非阻塞写入，避免文件 I/O 影响服务性能
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    // 文件日志层: JSON 格式，不带 ANSI 颜色码，便于日志收集系统解析
    let file_layer = fmt::layer()
        .with_writer(non_blocking)
        .with_ansi(false)
        .with_target(true)
        .with_thread_ids(true)
        .with_file(true)       // 添加文件名
        .with_line_number(true)  // 添加行号
        .with_thread_names(true)
        .with_thread_ids(true)
        .json();

    // 控制台日志层: 简洁格式，带颜色，便于开发调试
    let console_layer = fmt::layer()
        .with_writer(std::io::stdout)
        .with_ansi(true)
        .with_target(false)
        .with_file(true)       // 添加文件名
        .with_line_number(true)  // 添加行号
        .with_thread_names(true)
        .with_thread_ids(true)

        .compact();

    // 环境过滤层，读取 RUST_LOG 环境变量，默认 info 级别
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    // 注册日志层: 控制台 + 文件 + 环境过滤
    registry()
        .with(env_filter)
        .with(console_layer)
        .with(file_layer)
        .init();

    // 保持 guard 活跃直至程序结束，确保日志写入完成
    std::mem::forget(guard);

    // 初始化全局配置（含 Nacos 远程配置覆盖）
    init_global_config_with_nacos().await;
    // 初始化加密服务（从环境变量 CMX_ENCRYPT_KEY 读取密钥）
    cmx_utils::crypto::CryptoService::init_from_env();
    info!("加密服务初始化完成");

    // 初始化 Redis 缓存
    init_cache().await;
    // 初始化数据库数据源（内部会在注册连接后自动执行数据库迁移）
    init_datasources().await;

    // 初始化文件存储服务（必须在 init_datasources 之后）
    init_storage().await;

    // 获取 Web 服务器配置
    let web_config = web_config();

    // 初始化调试会话管理器
    cmx_debug::init();
    info!("调试会话管理器初始化完成");

    // 初始化 WASM 运行时（必须在 init_plugins 之前）
    init_runtime().await;

    // 初始化全局事件总线（必须在 init_plugins 之前）
    cmx_traits::GlobalEventBus::initialize().expect("初始化全局事件总线失败");
    info!("全局事件总线初始化完成");



    // 初始化服务管理器
    init_services().await;

    // 初始化插件管理器
    init_plugins().await;

    // 构建完整的 AppState（注入 trait 实例）
    let app_state = CmxAppState::new()
        .with_plugin_query(cmx_plugin::GlobalPluginManager::get_as_plugin_query())
        .with_runtime_invoker(cmx_runtime::GlobalExtismEngine::get_as_invoker())
        .with_service_query(GlobalServiceQuery::get().clone())
        .with_service_storage(GlobalServiceStorage::get().clone())
        .with_storage_service(cmx_storage::global::GlobalStorageService::get().service().clone());

    // -- 配置 API 路由
    let api_routes = routes::routes().with_state(app_state);

    // -- 使用中间件构建路由
    // 中间件顺序 (从外到内):
    // 1. CookieManager - 处理 cookies
    // 2. mw_req_stamp_resolver - 添加请求时间戳
    let routes_all = Router::new()
        .nest("/api", api_routes)
        .merge(routes::get_swagger_routes())
        .layer(CookieManagerLayer::new())
        .layer(middleware::from_fn(mw_context_resolver))
        .layer(middleware::from_fn(mw_trace))
        // 允许最大请求体100 MB
        .layer(RequestBodyLimitLayer::new(100 * 1024 * 1024))
        .layer(DefaultBodyLimit::max(100 * 1024 * 1024))
        // .layer(TraceLayer::new_for_http())
        // 3. CORS - 允许跨域请求
        .layer(cors_layer())
        ;
    // 应用剩余的中间件并添加静态文件服务
    let routes_all = routes_all.fallback_service(axum::routing::get_service(
        tower_http::services::ServeDir::new(&web_config.web_folder),
    ));

    // 绑定 TCP 监听器
    let listener = TcpListener::bind("0.0.0.0:8080")
        .await
        .map_err(|e| Error::ServerSetup(format!("Failed to bind address: {}", e)))?;

    // 启动服务器（带优雅关闭支持）
    info!("{}", "=".repeat(60));
    info!("🚀 {:<44} 🚀", "Web 服务器启动成功");
    info!("{}", "=".repeat(60));
    info!("   监听地址：{:?}", listener.local_addr().unwrap());
    info!("   静态文件目录：{}", web_config.web_folder);
    info!("   日志目录：{}", log_dir);
    info!("{}", "-".repeat(60));

    // 使用 with_graceful_shutdown 监听 Ctrl+C 信号
    axum::serve(listener, routes_all.into_make_service())
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(|e| Error::ServerSetup(format!("Server failed: {}", e)))?;

    // 优雅关闭：从 Nacos 注销服务实例
    info!("开始优雅关闭...");
    shutdown_nacos().await;
    info!("服务已优雅关闭");

    Ok(())
}

/// 监听优雅关闭信号
///
/// 监听 Ctrl+C (SIGINT) 和 SIGTERM 信号，
/// 收到信号后触发 graceful shutdown
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
