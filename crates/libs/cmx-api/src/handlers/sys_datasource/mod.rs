//! SysDatasource 模块
//!
//! 提供数据源实体的 CRUD 操作和动态管理功能
//! Entity/BMC/Filter/Service 已迁移至 cmx-biz crate，此处通过 re-export 保持兼容

pub mod handler;

// 从 cmx-biz re-export 业务层类型
pub use cmx_biz::datasource::{
    SysDatasource, SysDatasourceBmc, SysDatasourceFilter, SysDatasourceForCreate,
    SysDatasourceForUpdate, SysDatasourceService,
};

use crate::app_state::CmxAppState;
use crate::routes::traits::ModuleRoutes;
use axum::Router;
use axum::routing::{get, post};

/// SysDatasource 模块路由
pub struct SysDatasourceModule;

impl ModuleRoutes for SysDatasourceModule {
    fn routes(self) -> Router<CmxAppState> {
        let router = Router::new();
        // 注册 SysDatasource CRUD 路由
        let router =
            crate::register_crud_handlers_module!(router, sys_datasource_crud, "/sys-datasource");
        // 注册 SysDatasource 自定义路由
        router
            .route(
                "/sys-datasource/create-custom",
                post(handler::create_datasource),
            )
            .route(
                "/sys-datasource/update-custom",
                post(handler::update_datasource),
            )
            .route(
                "/sys-datasource/delete-custom",
                post(handler::delete_datasource),
            )
            .route("/sys-datasource/by-db-id", post(handler::get_by_db_id))
            .route(
                "/sys-datasource/test-connection",
                get(handler::test_connection),
            )
    }

    fn prefix() -> &'static str {
        "sys-datasource"
    }

    fn module_name(&self) -> &'static str {
        "sys_datasource"
    }
}
