//! 用户模块路由注册

pub mod audit_handler;
pub mod handler;
pub mod temp_role_handler;

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
            // 临时角色授权（阶段1新增）
            .route(
                "/iam/users/assign-temp-role",
                post(temp_role_handler::assign_temp_role),
            )
            .route(
                "/iam/users/revoke-temp-role",
                post(temp_role_handler::revoke_temp_role),
            )
            .route(
                "/iam/users/revoke-temp-roles-batch",
                post(temp_role_handler::revoke_temp_roles_batch),
            )
            .route(
                "/iam/users/extend-temp-role",
                post(temp_role_handler::extend_temp_role),
            )
            .route(
                "/iam/users/temp-assignments",
                get(temp_role_handler::get_temp_assignments),
            )
            // 角色被授权用户列表（临时授权查询，按 role_id 查询，复用 get_temp_assignments）
            .route(
                "/iam/roles/temp-assigned-users",
                get(temp_role_handler::get_temp_assignments),
            )
            // 审计查询（阶段5新增）
            .route(
                "/iam/users/effective-permissions",
                get(audit_handler::get_effective_permissions),
            )
    }

    fn prefix() -> &'static str {
        "iam/users"
    }

    fn module_name(&self) -> &'static str {
        "iam/user"
    }
}
