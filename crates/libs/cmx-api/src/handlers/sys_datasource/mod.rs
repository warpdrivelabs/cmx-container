//! SysDatasource 模块
//!
//! 提供数据源实体的 CRUD 操作和动态管理功能

mod bmc;
mod entity;
mod filter;
pub mod handler;
pub mod service;

pub use bmc::SysDatasourceBmc;
pub use entity::{SysDatasource, SysDatasourceForCreate, SysDatasourceForUpdate};
pub use filter::SysDatasourceFilter;
pub use service::SysDatasourceService;

use crate::app_state::CmxAppState;
use crate::routes::traits::ModuleRoutes;
use axum::routing::{get, post};
use axum::Router;

/// SysDatasource 模块路由
pub struct SysDatasourceModule;

impl ModuleRoutes for SysDatasourceModule {
    fn routes(self) -> Router<CmxAppState> {
        let router = Router::new();
        // 注册 SysDatasource CRUD 路由
        let router = crate::register_crud_handlers_module!(router, sys_datasource_crud, "/sys-datasource");
        // 注册 SysDatasource 自定义路由
        router
            .route("/sys-datasource/create-custom", post(handler::create_datasource))
            .route("/sys-datasource/update-custom", post(handler::update_datasource))
            .route("/sys-datasource/delete-custom", post(handler::delete_datasource))
            .route("/sys-datasource/by-db-id", post(handler::get_by_db_id))
            .route("/sys-datasource/test-connection", get(handler::test_connection))
    }

    fn prefix() -> &'static str {
        "sys-datasource"
    }

    fn module_name(&self) -> &'static str {
        "sys_datasource"
    }
}
