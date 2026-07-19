//! Module 模块
//!
//! 提供模块实体的 CRUD 操作与迁移包导入/导出。
//! 写操作（create/update/delete）手写委托 ModuleService（带 DAM 资产文件副作用钩子）；
//! 读操作（get/list/page）复用 rest::handler 泛型函数。
//! Entity/BMC/Filter/Service 已迁移至 cmx-biz crate，此处通过 re-export 保持兼容。

pub mod handler;
pub mod package_handler;

// 从 cmx-biz re-export 业务层类型
pub use cmx_biz::module::{
    Module, ModuleBmc, ModuleFilter, ModuleForCreate, ModuleForUpdate, ModuleService,
};

pub use handler::module_custom_page;

use crate::app_state::CmxAppState;
use crate::rest::handler as rest_handler;
use crate::routes::traits::ModuleRoutes;
use axum::Router;
use axum::routing::{get, post};

/// Module 模块路由
pub struct ModuleHandler;

impl ModuleRoutes for ModuleHandler {
    fn routes(self) -> Router<CmxAppState> {
        Router::new()
            // 写操作：手写，走 ModuleService（带 DAM 资产钩子）
            .route("/module/create", post(handler::create_module))
            .route("/module/update", post(handler::update_module))
            .route("/module/delete", post(handler::delete_module))
            // 读操作：复用 rest::handler 泛型函数（无副作用）
            .route("/module/get", get(rest_handler::get_by_id::<ModuleBmc>))
            .route(
                "/module/list",
                post(rest_handler::list::<ModuleBmc, ModuleFilter>),
            )
            .route(
                "/module/page",
                post(rest_handler::page::<ModuleBmc, ModuleFilter>),
            )
            // 自定义：联表分页（带 application_name + domain_name）
            .route("/module/custom-page", post(module_custom_page))
            // 模块迁移包导入/导出
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
        "module"
    }

    fn module_name(&self) -> &'static str {
        "module"
    }
}
