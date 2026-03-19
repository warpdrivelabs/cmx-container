//! Web 服务器主模块
//!
//! 该模块是应用程序的入口点，负责初始化各种组件、配置路由并启动 HTTP 服务器

mod config;
mod error;
mod routes;
pub use self::error::{Error, Result};
use config::web_config;

use axum::{Router, middleware};

use crate::config::{init_cache, init_db_datasource, init_global_config};
use cmx_api::CmxAppState;
use cmx_api::middleware::{cors_layer, mw_svr_context_resolver};
use cmx_database::get_default_db_manager;
use tokio::net::TcpListener;
use tower_cookies::CookieManagerLayer;
use tracing::info;
use tracing_subscriber::EnvFilter;

/// 应用程序主函数
///
/// 负责初始化配置、设置日志、创建模型管理器、初始化连接池、配置路由并启动服务器
///
/// # 返回值
/// - `Result<()>` - 执行结果，成功返回 Ok(())，失败返回错误
#[tokio::main]
async fn main() -> Result<()> {

    // 配置日志系统
    tracing_subscriber::fmt()
        // .without_time() // 用于早期本地开发
        .with_target(false)
        // 使用环境变量过滤器来控制哪些日志级别和模块的日志会被输出
        // 它会读取 RUST_LOG 环境变量来确定日志过滤规则
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();
    //初始化全局配置
    init_global_config();
    //初始化数据库数据源
    init_db_datasource().await;
    //初始化redis缓存
    init_cache().await;
    // 获取 Web 服务器配置
    let web_config = web_config();

    // 初始化数据库管理器（用于初始化连接池）
    let _database_manager = get_default_db_manager();

    // //获取默认的数据库ID
    // let default_db_id = get_default_db_manager().get_default_db_id().await;

    // -- 配置 API 路由
    let api_routes = self::routes::routes().with_state(CmxAppState::new());

    // -- 使用中间件构建路由
    // 中间件顺序 (从外到内):
    // 1. CookieManager - 处理 cookies
    // 2. mw_req_stamp_resolver - 添加请求时间戳
    let routes_all = Router::new()
        .nest("/api", api_routes)
        .layer(CookieManagerLayer::new())
        .layer(middleware::from_fn(mw_svr_context_resolver));
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
    info!(
        "   监听地址：{}",
        format!("{:?}", listener.local_addr().unwrap())
    );
    info!("   静态文件目录：{}", web_config.WEB_FOLDER);
    info!("{}", "-".repeat(60));

    // 启动服务器
    axum::serve(listener, routes_all.into_make_service())
        .await
        .map_err(|e| Error::ServerSetup(format!("Server failed: {}", e)))?;

    Ok(())
}
