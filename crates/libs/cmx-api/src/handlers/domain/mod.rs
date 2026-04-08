//! Domain 模块
//!
//! 提供领域实体的 CRUD 操作

mod bmc;
mod entity;
mod filter;
pub mod handler;
mod service;

pub use bmc::DomainBmc;
pub use entity::{Domain, DomainForCreate, DomainForUpdate, DomainTreeNodeData};
pub use filter::DomainFilter;
pub use service::DomainService;

use crate::app_state::CmxAppState;
use crate::routes::traits::ModuleRoutes;
use axum::routing::post;
use axum::Router;

/// Domain 模块路由
pub struct DomainModule;

impl ModuleRoutes for DomainModule {
    fn routes(self) -> Router<CmxAppState> {
        let router = Router::new();
        // 注册 Domain CRUD 路由
        let router = crate::register_crud_handlers_module!(router, domain_crud, "/domains");
        // 注册 Domain 自定义路由
        router.route("/domains/tree", post(handler::get_tree))
    }

    fn prefix() -> &'static str {
        "domains"
    }

    fn module_name(&self) -> &'static str {
        "domain"
    }
}
