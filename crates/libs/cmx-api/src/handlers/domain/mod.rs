//! Domain 模块
//!
//! 提供领域实体的 CRUD 操作
//! Entity/BMC/Filter/Service 已迁移至 cmx-biz crate，此处通过 re-export 保持兼容

pub mod handler;

// 从 cmx-biz re-export 业务层类型
pub use cmx_biz::domain::{
    Domain, DomainBmc, DomainFilter, DomainForCreate, DomainForUpdate, DomainService,
    DomainTreeNodeData,
};

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
