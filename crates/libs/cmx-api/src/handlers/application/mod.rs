//! Application 模块
//!
//! 提供应用实体的 CRUD 操作
//! Entity/BMC/Filter 已迁移至 cmx-biz crate，此处通过 re-export 保持兼容

pub mod handler;

// 从 cmx-biz re-export 业务层类型
pub use cmx_biz::application::{
    Application, ApplicationBmc, ApplicationFilter, ApplicationForCreate, ApplicationForUpdate,
};

pub use handler::application_custom_page;

use crate::app_state::CmxAppState;
use crate::routes::traits::ModuleRoutes;
use axum::Router;
use axum::routing::post;

/// Application 模块路由
pub struct ApplicationModule;

impl ModuleRoutes for ApplicationModule {
    fn routes(self) -> Router<CmxAppState> {
        let router = Router::new();
        // 注册 Application CRUD 路由
        let router =
            crate::register_crud_handlers_module!(router, application_crud, "/applications");
        // 注册 Application 自定义路由
        router.route("/applications/custom-page", post(application_custom_page))
    }

    fn prefix() -> &'static str {
        "applications"
    }

    fn module_name(&self) -> &'static str {
        "application"
    }
}
