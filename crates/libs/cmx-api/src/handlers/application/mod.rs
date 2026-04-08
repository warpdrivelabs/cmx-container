//! Application 模块
//!
//! 提供应用实体的 CRUD 操作

mod bmc;
mod entity;
mod filter;
pub mod handler;

pub use bmc::ApplicationBmc;
pub use entity::{Application, ApplicationForCreate, ApplicationForUpdate};
pub use filter::ApplicationFilter;
pub use handler::application_custom_page;

use crate::app_state::CmxAppState;
use crate::routes::traits::ModuleRoutes;
use axum::routing::post;
use axum::Router;

/// Application 模块路由
pub struct ApplicationModule;

impl ModuleRoutes for ApplicationModule {
    fn routes(self) -> Router<CmxAppState> {
        let router = Router::new();
        // 注册 Application CRUD 路由
        let router = crate::register_crud_handlers_module!(router, application_crud, "/applications");
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
