//! 权限规则模块路由注册

pub mod handler;

use crate::app_state::CmxAppState;
use crate::routes::traits::ModuleRoutes;
use axum::routing::{get, post};
use axum::Router;

/// 权限规则模块路由
pub struct RuleModule;

impl ModuleRoutes for RuleModule {
    fn routes(self) -> Router<CmxAppState> {
        Router::new()
            // CRUD
            .route("/iam/permission-rules/create", post(handler::create_rule))
            .route("/iam/permission-rules/update/{rule_id}", post(handler::update_rule))
            .route("/iam/permission-rules/delete/{rule_id}", post(handler::delete_rule))
            .route("/iam/permission-rules/get/{rule_id}", get(handler::get_rule))
            // 分页查询
            .route("/iam/permission-rules/page", post(handler::page_rules))
            // 启用/禁用
            .route(
                "/iam/permission-rules/toggle-status",
                post(handler::toggle_rule_status),
            )
            // 规则项管理
            .route(
                "/iam/permission-rules/items/add",
                post(handler::add_rule_items),
            )
            .route(
                "/iam/permission-rules/items/remove",
                post(handler::remove_rule_items),
            )
            // 规则校验测试
            .route("/iam/permission-rules/validate", post(handler::validate_rule))
    }

    fn prefix() -> &'static str {
        "iam/permission-rules"
    }

    fn module_name(&self) -> &'static str {
        "iam/rule"
    }
}
