//! 路由注册宏
//!
//! 提供宏来简化通用 CRUD 路由的注册。

/// 注册通用 CRUD 路由
///
/// # 参数
/// * `router` - Axum 路由器
/// * `mc` - 模型控制器类型（实现 DbBmc trait）
/// * `filter` - 过滤器类型（实现 Into<FilterGroups>）
/// * `prefix` - 路由前缀
///
/// # 示例
/// ```
/// let app = Router::new()
///     .with_state(mm);
/// let app = register_crud_routes!(app, DomainBmc, DomainFilter, "/api/domains");
/// ```
#[macro_export]
macro_rules! register_crud_routes {
    ($router:expr, $mc:ty, $filter:ty, $prefix:expr) => {
        $router
            .route(concat!($prefix, "/create"), axum::routing::post($crate::rest::handler::create::<$mc>))
            .route(concat!($prefix, "/get"), axum::routing::get($crate::rest::handler::get_by_id::<$mc>))
            .route(concat!($prefix, "/update"), axum::routing::post($crate::rest::handler::update::<$mc>))
            .route(concat!($prefix, "/delete"), axum::routing::get($crate::rest::handler::delete_by_id::<$mc>))
            .route(concat!($prefix, "/list"), axum::routing::post($crate::rest::handler::list::<$mc, $filter>))
            .route(concat!($prefix, "/page"), axum::routing::post($crate::rest::handler::page::<$mc, $filter>))
    };
}
