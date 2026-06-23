//! 角色组模块路由注册

pub mod handler;

use crate::app_state::CmxAppState;
use crate::routes::traits::ModuleRoutes;
use axum::routing::{get, post};
use axum::Router;

/// 角色组模块路由
pub struct RoleGroupModule;

impl ModuleRoutes for RoleGroupModule {
    fn routes(self) -> Router<CmxAppState> {
        Router::new()
            // 角色组 CRUD
            .route("/iam/role-groups/create", post(handler::create_role_group))
            .route("/iam/role-groups/update", post(handler::update_role_group))
            .route("/iam/role-groups/delete", post(handler::delete_role_group))
            .route("/iam/role-groups/get", get(handler::get_role_group))
            .route("/iam/role-groups/page", post(handler::page_role_groups))
            .route("/iam/role-groups/list", post(handler::list_role_groups))
            // 角色组树
            .route("/iam/role-groups/tree", get(handler::get_role_group_tree))
    }

    fn prefix() -> &'static str {
        "iam/role-groups"
    }

    fn module_name(&self) -> &'static str {
        "iam/role_group"
    }
}
