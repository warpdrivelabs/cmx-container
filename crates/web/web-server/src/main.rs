//! Web 服务器主模块
//!
//! 该模块是应用程序的入口点，负责初始化各种组件、配置路由并启动 HTTP 服务器

mod config;
mod error;
mod plugins;
mod routes;

pub use self::error::{Error, Result};
use config::web_config;

use axum::{middleware, Router};

use crate::config::{init_cache, init_db_datasource, init_global_config, init_plugins};
use cmx_api::middleware::{mw_context_resolver, mw_trace};
use cmx_api::CmxAppState;
use tokio::net::TcpListener;
use tower_cookies::CookieManagerLayer;
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
        .json();

    // 控制台日志层: 简洁格式，带颜色，便于开发调试
    let console_layer = fmt::layer()
        .with_writer(std::io::stdout)
        .with_ansi(true)
        .with_target(false)
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

    // 初始化全局配置
    init_global_config();
    // 初始化数据库数据源
    init_db_datasource().await;
    // 初始化 Redis 缓存
    init_cache().await;
    // 获取 Web 服务器配置
    let web_config = web_config();

    init_plugins().await;

    // -- 配置 API 路由
    let api_routes = routes::routes().with_state(CmxAppState::new());

    // -- 使用中间件构建路由
    // 中间件顺序 (从外到内):
    // 1. CookieManager - 处理 cookies
    // 2. mw_req_stamp_resolver - 添加请求时间戳
    let routes_all = Router::new()
        .nest("/api", api_routes)
        .merge(routes::get_swagger_routes())
        .layer(CookieManagerLayer::new())
        .layer(middleware::from_fn(mw_context_resolver))
        // .layer(middleware::from_fn(mw_trace))
        ;
    // 应用剩余的中间件并添加静态文件服务
    let routes_all = routes_all.fallback_service(axum::routing::get_service(
        tower_http::services::ServeDir::new(&web_config.WEB_FOLDER),
    ));

    // 绑定 TCP 监听器
    let listener = TcpListener::bind("0.0.0.0:8080")
        .await
        .map_err(|e| Error::ServerSetup(format!("Failed to bind address: {}", e)))?;

    // 启动服务器
    info!("{}", "=".repeat(60));
    info!("🚀 {:<44} 🚀", "Web 服务器启动成功");
    info!("{}", "=".repeat(60));
    info!("   监听地址：{:?}", listener.local_addr().unwrap());
    info!("   静态文件目录：{}", web_config.WEB_FOLDER);
    info!("   日志目录：{}", log_dir);
    info!("{}", "-".repeat(60));

    // 启动服务器
    axum::serve(listener, routes_all.into_make_service())
        .await
        .map_err(|e| Error::ServerSetup(format!("Server failed: {}", e)))?;

    Ok(())
}
