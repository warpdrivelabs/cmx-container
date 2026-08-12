//! Module 模块（CRUD 部分）—— 从 cmx-api 拆分迁入。
//!
//! 写操作（create/update/delete）手写委托 ModuleService 以触发 DAM 资产文件副作用；
//! 读操作（get/list/page）复用 cmx-api-core 的 rest::handler 泛型函数。
//! Entity/BMC/Filter/Service 定义在 cmx-biz::module。
//!
//! 注：模块迁移包导入/导出（package_handler）已拆到 cmx-plugin-api（ModulePackageModule），
//! 因其依赖 cmx-plugin；本模块只依赖 cmx-biz，避免 biz⇄plugin 环。

pub mod handler;

// 从 cmx-biz re-export 业务层类型（crud 泛型路由 + openapi schema 用）
pub use cmx_biz::module::{
    Module, ModuleBmc, ModuleFilter, ModuleForCreate, ModuleForUpdate, ModuleService,
};

pub use handler::module_custom_page;

use axum::Router;
use axum::routing::{get, post};

use cmx_api_core::CmxAppState;
use cmx_api_core::ModuleRoutes;
use cmx_api_core::rest::handler as rest_handler;

/// Module CRUD 路由聚合（写手写委托 Service；读复用 rest::handler 泛型）。
pub struct ModuleCrudModule;

impl ModuleRoutes for ModuleCrudModule {
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
    }

    fn prefix() -> &'static str {
        "module"
    }

    fn module_name(&self) -> &'static str {
        "module-crud"
    }
}
