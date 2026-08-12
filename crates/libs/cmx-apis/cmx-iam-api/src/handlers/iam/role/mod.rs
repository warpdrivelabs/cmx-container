//! 角色模块路由注册

pub mod audit_handler;
pub mod handler;

use cmx_api_core::CmxAppState;
use cmx_api_core::ModuleRoutes;
use axum::Router;
use axum::routing::{get, post};

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
            // 给角色批量赋权（覆盖式：替换该角色的权限集合）
            .route(
                "/iam/roles/assign-permissions",
                post(handler::assign_permissions),
            )
            // 给角色批量分配用户
            .route("/iam/roles/assign-users", post(handler::assign_role_users))
            // 查询角色已绑定的权限清单
            .route("/iam/roles/permissions", get(handler::get_role_permissions))
            // 查询角色下的用户清单
            .route("/iam/roles/users", get(handler::get_role_users))
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
