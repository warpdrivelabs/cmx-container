//! Module 模块
//!
//! 提供模块实体的 CRUD 操作与迁移包导入/导出
//! Entity/BMC/Filter 已迁移至 cmx-biz crate，此处通过 re-export 保持兼容

pub mod handler;
pub mod package_handler;

// 从 cmx-biz re-export 业务层类型
pub use cmx_biz::module::{Module, ModuleBmc, ModuleFilter, ModuleForCreate, ModuleForUpdate};

pub use handler::module_custom_page;

use crate::app_state::CmxAppState;
use crate::routes::traits::ModuleRoutes;
use axum::routing::{get, post};
use axum::Router;

/// Module 模块路由
pub struct ModuleHandler;

impl ModuleRoutes for ModuleHandler {
    fn routes(self) -> Router<CmxAppState> {
        let router = Router::new();
        // 注册 Module CRUD 路由
        let router = crate::register_crud_handlers_module!(router, module_crud, "/module");
        // 注册 Module 自定义路由
        let router = router.route("/module/custom-page", post(module_custom_page));
        // 注册模块迁移包导入/导出路由
        router
            .route("/module/package/import", post(package_handler::module_package_import))
            .route("/module/package/export", get(package_handler::module_package_export))
    }

    fn prefix() -> &'static str {
        "module"
    }

    fn module_name(&self) -> &'static str {
        "module"
    }
}
