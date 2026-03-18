//! 路由注册宏
//!
//! 提供宏来简化通用 CRUD 路由的注册。

/// 注册通用 CRUD 路由
///
/// # 参数
/// * `router` - Axum 路由器
/// * `bmc` - 模型控制器类型（实现 DbBmc trait）
/// * `filter` - 过滤器类型（实现 Into<FilterGroups>）
/// * `entity_create` - 创建 Entity 类型（实现 HasSeaFields）
/// * `entity_update` - 更新 Entity 类型（实现 HasSeaFields）
/// * `prefix` - 路由前缀
///
/// # 示例
/// ```
/// let app = Router::new()
///     .with_state(state);
/// let app = register_crud_routes!(app, DomainBmc, DomainFilter, DomainForCreate, DomainForUpdate, "/domains");
/// ```
#[macro_export]
macro_rules! register_crud_routes {
    ($router:expr, $bmc:ty, $filter:ty, $entity_create:ty, $entity_update:ty, $prefix:expr) => {
        $router
            // 创建操作（使用 ForCreate）
            .route(concat!($prefix, "/create"), axum::routing::post($crate::rest::handler::create::<$bmc, $entity_create>))
            .route(concat!($prefix, "/create-many"), axum::routing::post($crate::rest::handler::create_many::<$bmc, $entity_create>))
            // 查询操作
            .route(concat!($prefix, "/get"), axum::routing::get($crate::rest::handler::get_by_id::<$bmc>))
            // 更新操作（使用 ForUpdate）
            .route(concat!($prefix, "/update"), axum::routing::post($crate::rest::handler::update::<$bmc, $entity_update>))
            .route(concat!($prefix, "/update-many"), axum::routing::post($crate::rest::handler::update_many::<$bmc, $entity_update>))
            // 删除操作（支持单个和批量）
            .route(concat!($prefix, "/delete"), axum::routing::post($crate::rest::handler::delete::<$bmc>))
            // 列表和分页查询
            .route(concat!($prefix, "/list"), axum::routing::post($crate::rest::handler::list::<$bmc, $filter>))
            .route(concat!($prefix, "/page"), axum::routing::post($crate::rest::handler::page::<$bmc, $filter>))
    };
}
