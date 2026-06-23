//! 角色模块路由注册

pub mod audit_handler;
pub mod handler;

use crate::app_state::CmxAppState;
use crate::routes::traits::ModuleRoutes;
use axum::routing::{get, post};
use axum::Router;

/// 角色模块路由
pub struct RoleModule;

impl ModuleRoutes for RoleModule {
    fn routes(self) -> Router<CmxAppState> {
        Router::new()
            // 角色 CRUD（RoleForCreate/RoleForUpdate derive Fields，可走通用 CRUD，但使用自定义 handler 统一风格）
            .route("/iam/roles/create", post(handler::create_role))
            .route("/iam/roles/update", post(handler::update_role))
            .route("/iam/roles/delete", post(handler::delete_role))
            .route("/iam/roles/get", get(handler::get_role))
            .route("/iam/roles/page", post(handler::page_roles))
            .route("/iam/roles/list", post(handler::list_roles))
            // 关联操作
            .route("/iam/roles/assign-permissions", post(handler::assign_permissions))
            .route("/iam/roles/permissions", get(handler::get_role_permissions))
            // 审计查询（阶段5新增）
            .route(
                "/iam/roles/permission-diff",
                get(audit_handler::get_permission_diff),
            )
    }

    fn prefix() -> &'static str {
        "iam/roles"
    }

    fn module_name(&self) -> &'static str {
        "iam/role"
    }
}
