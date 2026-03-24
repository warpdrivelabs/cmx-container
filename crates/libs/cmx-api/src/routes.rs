//! 集中路由注册模块
//!
//! 提供统一的路由注册入口，简化 web-server 的路由配置

use crate::models::application::{
    ApplicationBmc, ApplicationFilter, ApplicationForCreate, ApplicationForUpdate,
};
use crate::models::domain;
use crate::models::domain::{DomainBmc, DomainFilter, DomainForCreate, DomainForUpdate};
use crate::models::module::{ModuleBmc, ModuleFilter, ModuleForCreate, ModuleForUpdate};
use crate::models::sys_datasource;
use crate::models::sys_datasource::{
    SysDatasourceBmc, SysDatasourceFilter, SysDatasourceForCreate, SysDatasourceForUpdate,
};
use crate::register_crud_routes;
use crate::state::CmxAppState;
use axum::Router;

/// 注册所有 API 路由
///
/// # 参数
/// * 无
///
/// # 返回值
/// 返回配置好的 Axum 路由器
///
/// # 示例
/// ```rust
/// use cmx_api::routes::api_routes;
/// use cmx_api::CmxAppState;
///
/// let router = api_routes().with_state(CmxAppState::default());
/// ```
///
/// # 注册的路由
/// ## Domain CRUD
/// - POST /domains/create       - 创建单个
/// - POST /domains/create-many  - 批量创建
/// - GET  /domains/get          - 获取（?id=xxx）
/// - POST /domains/update       - 更新单个
/// - POST /domains/update-many  - 批量更新
/// - POST /domains/delete       - 删除（支持单个和批量）
/// - POST /domains/list         - 列表查询
/// - POST /domains/page         - 分页查询
///
/// ## SysDatasource CRUD（自定义）
/// - POST /sys-datasource/create-custom       - 创建数据源（自动注册）
/// - POST /sys-datasource/create-many-custom  - 批量创建数据源
/// - POST /sys-datasource/update-custom       - 更新数据源（自动重新注册）
/// - POST /sys-datasource/update-many-custom  - 批量更新数据源
/// - POST /sys-datasource/delete-custom       - 删除数据源（自动注销）
/// - POST /sys-datasource/by-db-id            - 按 db_id 查询
/// - GET  /sys-datasource/test-connection     - 测试连接
/// - GET  /sys-datasource/registered          - 列出已注册数据源
pub fn api_routes() -> Router<CmxAppState> {
    let router = Router::new();

    // 注册 Domain CRUD 路由
    let router = register_crud_routes!(
        router,
        DomainBmc,
        DomainFilter,
        DomainForCreate,
        DomainForUpdate,
        "/domains"
    );

    let router = register_crud_routes!(
        router,
        ApplicationBmc,
        ApplicationFilter,
        ApplicationForCreate,
        ApplicationForUpdate,
        "/applications"
    );

    let router = register_crud_routes!(
        router,
        ModuleBmc,
        ModuleFilter,
        ModuleForCreate,
        ModuleForUpdate,
        "/module"
    );

    // 注册 SysDatasource 标准 CRUD 路由
    let router = register_crud_routes!(
        router,
        SysDatasourceBmc,
        SysDatasourceFilter,
        SysDatasourceForCreate,
        SysDatasourceForUpdate,
        "/sys-datasource"
    );

    // 注册 Domain 自定义路由
    let router = router.route(
        "/domains/by-name",
        axum::routing::post(domain::handler::get_by_name),
    );

    // 注册 SysDatasource 自定义路由
    let router = router
        .route(
            "/sys-datasource/create-custom",
            axum::routing::post(sys_datasource::handler::create_datasource),
        )
        .route(
            "/sys-datasource/update-custom",
            axum::routing::post(sys_datasource::handler::update_datasource),
        )

        .route(
            "/sys-datasource/delete-custom",
            axum::routing::post(sys_datasource::handler::delete_datasource),
        )


        // .route(
        //     "/sys-datasource/update-many-custom",
        //     axum::routing::post(sys_datasource::handler::update_many_datasource),
        // )
        // .route(
        //     "/sys-datasource/create-many-custom",
        //     axum::routing::post(sys_datasource::handler::create_many_datasource),
        // )

        .route(
            "/sys-datasource/by-db-id",
            axum::routing::post(sys_datasource::handler::get_by_db_id),
        )
        .route(
            "/sys-datasource/test-connection",
            axum::routing::get(sys_datasource::handler::test_connection),
        )
        .route(
            "/sys-datasource/registered",
            axum::routing::get(sys_datasource::handler::list_registered),
        );

    // 注册其他模型的路由
    // let router = register_crud_routes!(router, UserBmc, UserFilter, UserForCreate, UserForUpdate, "/users");

    router
}
