//! Module 模块
//!
//! 提供模块实体的 CRUD 操作

mod bmc;
mod entity;
mod filter;
pub mod handler;

pub use bmc::ModuleBmc;
pub use entity::{Module, ModuleForCreate, ModuleForUpdate};
pub use filter::ModuleFilter;
pub use handler::module_custom_page;

use crate::app_state::CmxAppState;
use crate::routes::traits::ModuleRoutes;
use axum::routing::post;
use axum::Router;

/// Module 模块路由
pub struct ModuleHandler;

impl ModuleRoutes for ModuleHandler {
    fn routes(self) -> Router<CmxAppState> {
        let router = Router::new();
        // 注册 Module CRUD 路由
        let router = crate::register_crud_handlers_module!(router, module_crud, "/module");
        // 注册 Module 自定义路由
        router.route("/module/custom-page", post(module_custom_page))
    }

    fn prefix() -> &'static str {
        "module"
    }

    fn module_name(&self) -> &'static str {
        "module"
    }
}
