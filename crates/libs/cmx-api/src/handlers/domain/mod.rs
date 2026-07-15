//! Domain 模块
//!
//! 提供领域实体的 CRUD 操作。
//! 写操作（create/update/delete）手写委托 DomainService（带 DAM 资产文件副作用钩子）；
//! 读操作（get/list/page）复用 rest::handler 泛型函数。
//! Entity/BMC/Filter/Service 已迁移至 cmx-biz crate，此处通过 re-export 保持兼容。

pub mod handler;

// 从 cmx-biz re-export 业务层类型
pub use cmx_biz::domain::{
    Domain, DomainBmc, DomainFilter, DomainForCreate, DomainForUpdate, DomainService,
    DomainTreeNodeData,
};

use crate::app_state::CmxAppState;
use crate::rest::handler as rest_handler;
use crate::routes::traits::ModuleRoutes;
use axum::Router;
use axum::routing::{get, post};

/// Domain 模块路由
pub struct DomainModule;

impl ModuleRoutes for DomainModule {
    fn routes(self) -> Router<CmxAppState> {
        Router::new()
            // 写操作：手写，走 DomainService（带 DAM 资产钩子）
            .route("/domains/create", post(handler::create_domain))
            .route("/domains/update", post(handler::update_domain))
            .route("/domains/delete", post(handler::delete_domain))
            // 读操作：复用 rest::handler 泛型函数（无副作用）
            .route("/domains/get", get(rest_handler::get_by_id::<DomainBmc>))
            .route("/domains/list", post(rest_handler::list::<DomainBmc, DomainFilter>))
            .route("/domains/page", post(rest_handler::page::<DomainBmc, DomainFilter>))
            // 自定义：域-应用-模块树
            .route("/domains/tree", post(handler::get_tree))
    }

    fn prefix() -> &'static str {
        "domains"
    }

    fn module_name(&self) -> &'static str {
        "domain"
    }
}
