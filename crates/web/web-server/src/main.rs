//! Web 服务器主模块
//!
//! 该模块是应用程序的入口点，负责初始化各种组件、配置路由并启动 HTTP 服务器

mod config;
mod error;
pub use self::error::{Error, Result};
use config::web_config;

use axum::{middleware, Router};

use crate::config::{init_db_datasource, init_global_config, WebConfig};
use cmx_api::middleware::mw_req_stamp::mw_req_stamp_resolver;
use cmx_database::{get_default_db_manager, DatabaseManager};
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

    //初始化全局配置
    init_global_config();
    //初始化数据库数据源
    init_db_datasource().await;

    // 配置日志系统
    tracing_subscriber::fmt()
        // .without_time() // 用于早期本地开发
        .with_target(false)
        // 使用环境变量过滤器来控制哪些日志级别和模块的日志会被输出
        // 它会读取 RUST_LOG 环境变量来确定日志过滤规则
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info"))
        )        .init();


    // 初始化配置文件并 获取 Web 服务器配置
    let config = web_config();

    //初始化数据库管理器
    let database_manager = get_default_db_manager();





    //
    // // -- 定义路由 (通过 generate_rpc_routes! 验证处理函数注解)
    // // 注意: 每个 RPC 模块中的 generate_rpc_routes! 在运行时验证处理器
    // let routes_rpc = web::routes_rpc::routes(mm.clone(), valkey_pool.clone())
    //     .route_layer(middleware::from_fn(mw_ctx_require));
    //
    // // 配置 REST API 路由
    // let routes_rest = routes_rest::routes(mm.clone())
    //     .layer(middleware::from_fn(mw_rest_info))
    //     .route_layer(middleware::from_fn(mw_ctx_require));
    //
    // // 需要认证的用户管理路由
    // let routes_user_auth = routes_user::routes_auth(mm.clone())
    //     .route_layer(middleware::from_fn(mw_ctx_require));
    //
    // // -- 从代码同步权限到数据库
    // // 这确保权限表始终与代码定义的权限保持同步
    // let ctx = Ctx::root_ctx();
    // PermissionBmc::sync_from_registry(&ctx, &mm).await.map_err(|e| {
    //     tracing::error!("同步权限失败: {:?}", e);
    //     Error::PermissionSync(e.to_string())
    // })?;
    //
    // // 注意: 管理员角色绕过 Ctx::require_permission() 中的所有权限检查
    // // 无需向管理员角色分配单独权限
    //
    // // -- 使用中间件构建路由
    // // 中间件顺序 (从外到内):
    // // 1. mw_req_stamp_resolver - 添加请求时间戳
    // // 2. CookieManager - 处理 cookies
    // // 3. mw_ctx_resolver - 从 token 解析用户上下文
    // // 4. mw_permission_resolver - 加载用户权限 (可选 Valkey 缓存)
    // // 5. mw_reponse_map - 映射响应
    // let routes_all = Router::new()
    //
    //     .merge(routes_user::routes_public(mm.clone())) // 注册
    //     .nest("/api", routes_rpc)
    //     .nest("/api", routes_rest)
    //     .layer(middleware::map_response(mw_reponse_map));
    //
    // // 根据缓存配置添加权限解析中间件
    // let routes_all = if let Some(pool) = valkey_pool {
    //     routes_all.layer(middleware::from_fn_with_state(
    //         (mm.clone(), pool),
    //         mw_permission_resolver_with_cache,
    //     ))
    // } else {
    //     routes_all.layer(middleware::from_fn_with_state(
    //         mm.clone(),
    //         mw_permission_resolver,
    //     ))
    // };

    // -- 使用中间件构建路由
    // 中间件顺序 (从外到内):
    // 1. CookieManager - 处理 cookies
    // 2. mw_req_stamp_resolver - 添加请求时间戳
    let routes_all = Router::new()
        .layer(CookieManagerLayer::new())
        .layer(middleware::from_fn(mw_req_stamp_resolver));

    // 应用剩余的中间件并添加静态文件服务
    let routes_all = routes_all
        .fallback_service(axum::routing::get_service(
            tower_http::services::ServeDir::new(&config.WEB_FOLDER)
        ));

    // 绑定 TCP 监听器
    let listener = TcpListener::bind("0.0.0.0:8080")
        .await
        .map_err(|e| Error::ServerSetup(format!("Failed to bind address: {}", e)))?;

    // 启动服务器
    info!("{}", "=".repeat(60));
    info!("🚀 {:<44} 🚀", "Web 服务器启动成功");
    info!("{}", "=".repeat(60));
    info!("   监听地址：{}", format!("{:?}", listener.local_addr().unwrap()));
    info!("   静态文件目录：{}", config.WEB_FOLDER);
    info!("{}", "-".repeat(60));

    // 启动服务器
    axum::serve(listener, routes_all.into_make_service())
        .await
        .map_err(|e| Error::ServerSetup(format!("Server failed: {}", e)))?;

    Ok(())
}
