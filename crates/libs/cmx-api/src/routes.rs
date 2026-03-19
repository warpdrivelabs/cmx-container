//! 集中路由注册模块
//!
//! 提供统一的路由注册入口，简化 web-server 的路由配置

use crate::models::application::{
    ApplicationBmc, ApplicationFilter, ApplicationForCreate, ApplicationForUpdate,
};
use crate::models::domain;
use crate::models::domain::{DomainBmc, DomainFilter, DomainForCreate, DomainForUpdate};
use crate::models::module::{ModuleBmc, ModuleFilter, ModuleForCreate, ModuleForUpdate};
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

    let router = register_crud_routes!(
        router,
        SysDatasourceBmc,
        SysDatasourceFilter,
        SysDatasourceForCreate,
        SysDatasourceForUpdate,
        "/sys-datasource"
    );

    //注册自定义路由
    let router = router.route(
        "/domains/by-name",
        axum::routing::post(domain::handler::get_by_name),
    );

    // 注册其他模型的路由
    // let router = register_crud_routes!(router, UserBmc, UserFilter, UserForCreate, UserForUpdate, "/users");

    router
}
