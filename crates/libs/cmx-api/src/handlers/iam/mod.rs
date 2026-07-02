//! IAM 模块路由注册
//!
//! 用户/角色/权限/规则四组 handler 的路由聚合

pub mod permission;
pub mod role;
pub mod role_group;
pub mod rule;
pub mod user;

use crate::app_state::CmxAppState;
use crate::routes::traits::ModuleRoutes;
use axum::Router;

/// IAM 模块路由
pub struct IamModule;

impl ModuleRoutes for IamModule {
    fn routes(self) -> Router<CmxAppState> {
        let router = Router::new();
        let router = router.merge(user::UserModule.routes());
        let router = router.merge(role::RoleModule.routes());
        let router = router.merge(role_group::RoleGroupModule.routes());
        let router = router.merge(permission::PermissionModule.routes());

        router.merge(rule::RuleModule.routes())
    }

    fn prefix() -> &'static str {
        "iam"
    }

    fn module_name(&self) -> &'static str {
        "iam"
    }
}
