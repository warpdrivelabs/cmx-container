//! Menu 模块
//!
//! 提供菜单实体的 CRUD 操作与树形查询
//! Entity/BMC/Filter/Service 定义在 cmx-biz crate，此处通过 re-export 暴露

pub mod handler;

// 从 cmx-biz re-export 业务层类型
pub use cmx_biz::menu::{Menu, MenuBmc, MenuFilter, MenuForCreate, MenuForUpdate, MenuService};

use crate::app_state::CmxAppState;
use crate::routes::traits::ModuleRoutes;
use axum::routing::post;
use axum::Router;

/// Menu 模块路由
pub struct MenuModule;

impl ModuleRoutes for MenuModule {
    fn routes(self) -> Router<CmxAppState> {
        let router = Router::new();
        // 注册 Menu 标准 CRUD 路由
        let router = crate::register_crud_handlers_module!(router, menu_crud, "/menu");
        // 注册自定义路由
        router.route("/menu/tree", post(handler::menu_tree))
    }

    fn prefix() -> &'static str {
        "menu"
    }

    fn module_name(&self) -> &'static str {
        "menu"
    }
}
