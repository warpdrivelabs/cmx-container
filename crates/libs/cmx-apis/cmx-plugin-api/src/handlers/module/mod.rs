//! Module 模块（迁移包导入/导出部分）—— 从 cmx-api 拆分迁入。
//!
//! 模块迁移包的导入/导出，依赖 cmx-plugin（ModuleExportService / ModuleInstallService）。
//! CRUD 部分已拆到 cmx-biz-api（ModuleCrudModule）。

pub mod package_handler;

use axum::Router;
use axum::routing::{get, post};

use cmx_api_core::CmxAppState;
use cmx_api_core::ModuleRoutes;

/// Module 迁移包导入/导出路由聚合。
pub struct ModulePackageModule;

impl ModuleRoutes for ModulePackageModule {
    fn routes(self) -> Router<CmxAppState> {
        Router::new()
            .route(
                "/module/package/import",
                post(package_handler::module_package_import),
            )
            .route(
                "/module/package/export",
                get(package_handler::module_package_export),
            )
    }

    fn prefix() -> &'static str {
        "module-package"
    }

    fn module_name(&self) -> &'static str {
        "module-package"
    }
}
