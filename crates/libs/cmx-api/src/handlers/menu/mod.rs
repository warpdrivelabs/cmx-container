//! Menu 模块
//!
//! 提供菜单实体的 CRUD 操作与树形查询
//! Entity/BMC/Filter/Service 定义在 cmx-biz crate，此处通过 re-export 暴露

pub mod handler;

// 从 cmx-biz re-export 业务层类型
pub use cmx_biz::menu::{
    Menu, MenuBmc, MenuFilter, MenuForCreate, MenuForUpdate, MenuService, MenuTreeNodeData,
};

use crate::app_state::CmxAppState;
use crate::routes::traits::ModuleRoutes;
use axum::Router;
use axum::routing::{get, post};

/// Menu 模块路由
pub struct MenuModule;

impl ModuleRoutes for MenuModule {
    fn routes(self) -> Router<CmxAppState> {
        // 菜单增删改涉及树形字段(leaf/depth/parent_code/id_path/code_path)的组装与级联,
        // 不能使用标准 CRUD 宏(宏走 GenericCrudService 直接写库,绕过 MenuService),
        // 故全部手写委托 MenuService。
        Router::new()
            .route("/menu/create", post(handler::create_menu))
            .route("/menu/get", get(handler::get_menu))
            .route("/menu/update", post(handler::update_menu))
            .route("/menu/delete", post(handler::delete_menu))
            .route("/menu/list", post(handler::list_menus))
            .route("/menu/page", post(handler::page_menus))
            .route("/menu/tree", get(handler::get_menu_tree))
    }

    fn prefix() -> &'static str {
        "menu"
    }

    fn module_name(&self) -> &'static str {
        "menu"
    }
}
