//! 用户模块路由注册

pub mod handler;

use crate::app_state::CmxAppState;
use crate::routes::traits::ModuleRoutes;
use axum::routing::{get, post};
use axum::Router;

/// 用户模块路由
pub struct UserModule;

impl ModuleRoutes for UserModule {
    fn routes(self) -> Router<CmxAppState> {
        Router::new()
            // 自定义 handler（UserForCreate/UserForUpdate 不 derive Fields，需 Service 层转换）
            .route("/iam/users/create", post(handler::create_user))
            .route("/iam/users/update", post(handler::update_user))
            .route("/iam/users/delete", post(handler::delete_user))
            .route("/iam/users/get", get(handler::get_user))
            .route("/iam/users/page", post(handler::page_users))
            .route("/iam/users/list", post(handler::list_users))
            // 关联操作
            .route("/iam/users/assign-roles", post(handler::assign_roles))
            .route("/iam/users/roles", get(handler::get_user_roles))
    }

    fn prefix() -> &'static str {
        "iam/users"
    }

    fn module_name(&self) -> &'static str {
        "iam/user"
    }
}
