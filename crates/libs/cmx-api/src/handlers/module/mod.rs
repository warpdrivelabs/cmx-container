//! Module 模块
//!
//! 提供模块实体的 CRUD 操作
//! Entity/BMC/Filter 已迁移至 cmx-biz crate，此处通过 re-export 保持兼容

pub mod handler;

// 从 cmx-biz re-export 业务层类型
pub use cmx_biz::module::{Module, ModuleBmc, ModuleFilter, ModuleForCreate, ModuleForUpdate};

pub use handler::module_custom_page;

use crate::app_state::CmxAppState;
use crate::routes::traits::ModuleRoutes;
use axum::Router;
use axum::routing::post;

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
