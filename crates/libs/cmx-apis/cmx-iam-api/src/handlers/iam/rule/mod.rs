//! 互斥规则模块路由注册

pub mod handler;

use cmx_api_core::CmxAppState;
use cmx_api_core::ModuleRoutes;
use axum::Router;
use axum::routing::{get, post};

/// 互斥规则模块路由
pub struct RuleModule;

impl ModuleRoutes for RuleModule {
    fn routes(self) -> Router<CmxAppState> {
        Router::new()
            // CRUD
            .route("/iam/exclusion-rules/create", post(handler::create_rule))
            .route(
                "/iam/exclusion-rules/update/{rule_id}",
                post(handler::update_rule),
            )
            .route(
                "/iam/exclusion-rules/delete/{rule_id}",
                post(handler::delete_rule),
            )
            .route("/iam/exclusion-rules/get/{rule_id}", get(handler::get_rule))
            // 分页查询
            .route("/iam/exclusion-rules/page", post(handler::page_rules))
            // 启用/禁用
            .route(
                "/iam/exclusion-rules/toggle-status",
                post(handler::toggle_rule_status),
            )
            // 互斥对象管理
            .route(
                "/iam/exclusion-rules/items/add",
                post(handler::add_rule_items),
            )
            .route(
                "/iam/exclusion-rules/items/remove",
                post(handler::remove_rule_items),
            )
            // 规则校验测试
            .route(
                "/iam/exclusion-rules/validate",
                post(handler::validate_rule),
            )
    }

    fn prefix() -> &'static str {
        "iam/exclusion-rules"
    }

    fn module_name(&self) -> &'static str {
        "iam/rule"
    }
}
