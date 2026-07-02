//! 权限模块路由注册

pub mod audit_handler;
pub mod handler;
pub mod import_handler;

use crate::app_state::CmxAppState;
use crate::routes::traits::ModuleRoutes;
use axum::Router;
use axum::routing::{get, post};

/// 权限模块路由
pub struct PermissionModule;

impl ModuleRoutes for PermissionModule {
    fn routes(self) -> Router<CmxAppState> {
        Router::new()
            // 权限 CRUD
            .route("/iam/permissions/create", post(handler::create_permission))
            .route("/iam/permissions/update", post(handler::update_permission))
            .route("/iam/permissions/delete", post(handler::delete_permission))
            .route("/iam/permissions/get", get(handler::get_permission))
            .route("/iam/permissions/page", post(handler::page_permissions))
            .route("/iam/permissions/list", post(handler::list_permissions))
            // 权限树
            .route("/iam/permissions/tree", get(handler::get_permission_tree))
            // 审计查询（阶段5新增）
            .route(
                "/iam/permissions/usage-stat",
                get(audit_handler::get_permission_usage_stat),
            )
            // 插件数据导入/清理（权限中心数据集成）
            .route(
                "/iam/permissions/import",
                post(import_handler::import_permissions),
            )
            .route(
                "/iam/permissions/cleanup",
                post(import_handler::cleanup_permissions),
            )
    }

    fn prefix() -> &'static str {
        "iam/permissions"
    }

    fn module_name(&self) -> &'static str {
        "iam/permission"
    }
}
