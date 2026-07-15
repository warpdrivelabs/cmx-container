//! Application 模块
//!
//! 提供应用实体的 CRUD 操作。
//! 写操作（create/update/delete）手写委托 ApplicationService（带 DAM 资产文件副作用钩子）；
//! 读操作（get/list/page）复用 rest::handler 泛型函数。
//! Entity/BMC/Filter/Service 已迁移至 cmx-biz crate，此处通过 re-export 保持兼容。

pub mod handler;

// 从 cmx-biz re-export 业务层类型
pub use cmx_biz::application::{
    Application, ApplicationBmc, ApplicationFilter, ApplicationForCreate, ApplicationForUpdate,
    ApplicationService,
};

pub use handler::application_custom_page;

use crate::app_state::CmxAppState;
use crate::rest::handler as rest_handler;
use crate::routes::traits::ModuleRoutes;
use axum::Router;
use axum::routing::{get, post};

/// Application 模块路由
pub struct ApplicationModule;

impl ModuleRoutes for ApplicationModule {
    fn routes(self) -> Router<CmxAppState> {
        Router::new()
            // 写操作：手写，走 ApplicationService（带 DAM 资产钩子）
            .route("/applications/create", post(handler::create_application))
            .route("/applications/update", post(handler::update_application))
            .route("/applications/delete", post(handler::delete_application))
            // 读操作：复用 rest::handler 泛型函数（无副作用）
            .route("/applications/get", get(rest_handler::get_by_id::<ApplicationBmc>))
            .route("/applications/list", post(rest_handler::list::<ApplicationBmc, ApplicationFilter>))
            .route("/applications/page", post(rest_handler::page::<ApplicationBmc, ApplicationFilter>))
            // 自定义：联表分页（带 domain_name）
            .route("/applications/custom-page", post(application_custom_page))
    }

    fn prefix() -> &'static str {
        "applications"
    }

    fn module_name(&self) -> &'static str {
        "application"
    }
}
