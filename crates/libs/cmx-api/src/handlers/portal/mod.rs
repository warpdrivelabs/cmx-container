//! 门户/设计器业务 API 模块（迁移自 CMXPortalManager / CMXHTMLDesigner 的 Node 后端）。
//!
//! 路由路径与 Node 后端保持一致（挂在 `/api` 下），响应统一用 [`crate::ApiResp`] 信封。
//! Handler 按功能拆分为独立子模块，路由按功能分组注册。

pub mod ai;
pub mod data;
pub mod definitions;
pub mod flexible_combination;
pub mod launcher;
pub mod legacy;
pub mod meta;
pub mod model;
pub mod notify;
pub mod pages;
pub mod registry;

// 模型中心已抽出为独立 crate cmx-model-center。
// 再导出为 `model_center`，保持 model.rs 里 `super::model_center::…` 的调用路径零改动。
pub use cmx_model_center as model_center;

use axum::Router;
use axum::routing::{get, post};

use crate::app_state::CmxAppState;
use crate::routes::traits::ModuleRoutes;

/// 门户业务模块路由聚合。
pub struct PortalModule;

impl ModuleRoutes for PortalModule {
    fn routes(self) -> Router<CmxAppState> {
        Router::new()
            .merge(ai_routes())
            .merge(meta_routes())
            .merge(pages_routes())
            .merge(data_routes())
            .merge(notify_routes())
            .merge(launcher_routes())
            .merge(registry_routes())
            .merge(definitions_routes())
            .merge(flexible_combination_routes())
            .merge(model_routes())
    }

    fn prefix() -> &'static str {
        "portal"
    }

    fn module_name(&self) -> &'static str {
        "portal"
    }
}

// ─── AI 对话中继 + 本地编辑代理 ───
fn ai_routes() -> Router<CmxAppState> {
    Router::new()
        .route("/ai/chat", post(ai::ai_chat))
        .route("/agent/capabilities", get(ai::agent_capabilities))
        .route("/agent/message", post(ai::agent_message))
        .route("/agent/message/stream", post(ai::agent_message_stream))
        .route("/agent/approvals/{id}", post(ai::agent_approval))
}

// ─── 域 / 菜单 / 活动 / 工作区节点 ───
fn meta_routes() -> Router<CmxAppState> {
    Router::new()
        .route("/domains", get(meta::get_domains))
        .route("/menu-pages", get(meta::get_menu_pages))
        .route("/activities", get(meta::get_activities))
        .route(
            "/workspace-nodes",
            get(meta::list_workspace_nodes).post(meta::save_workspace_node),
        )
        .route(
            "/workspace-nodes/{id}",
            get(meta::get_workspace_node).delete(meta::delete_workspace_node),
        )
}

// ─── 表单页 / 原生页面 / HTML 页面 ───
fn pages_routes() -> Router<CmxAppState> {
    Router::new()
        .route(
            "/form-pages",
            get(pages::list_form_pages).post(pages::save_form_page),
        )
        .route("/form-pages/{id}", get(pages::get_form_page))
        .route(
            "/native-pages",
            get(pages::list_native_pages).post(pages::save_native_page),
        )
        .route("/native-pages/batch", post(pages::batch_native_pages))
        .route("/native-pages/{id}", get(pages::get_native_page))
        .route(
            "/html-pages",
            get(pages::list_html_pages).post(pages::save_html_page),
        )
        .route("/html-pages/batch", post(pages::batch_html_pages))
        .route("/html-pages/{id}", get(pages::get_html_page))
}

// ─── 事实数据 / 帮助中心 ───
fn data_routes() -> Router<CmxAppState> {
    Router::new()
        .route("/fact/list", get(data::list_facts))
        .route("/fact/get", post(data::get_fact_post))
        .route(
            "/fact/{domain}/{app}/{module}/{file}",
            get(data::get_fact_path),
        )
        .route("/help/catalog", get(data::help_catalog))
        .route("/help/get", post(data::help_get_post))
        .route("/help/doc", post(data::help_save_doc))
        .route(
            "/help/doc/{domain}/{app}/{module}/{file}",
            get(data::help_get_path).delete(data::help_delete_doc),
        )
}

// ─── 通知中心（任务/消息/日志 + SSE 主动推送）───
fn notify_routes() -> Router<CmxAppState> {
    Router::new()
        .route("/notifications", get(notify::notify_list))
        .route("/notifications/centers", get(notify::notify_centers))
        .route("/notifications/counts", get(notify::notify_counts))
        .route("/notifications/publish", post(notify::notify_publish))
        .route("/notifications/mark-read", post(notify::notify_mark_read))
        .route("/notifications/stream", get(notify::notify_stream))
}

// ─── 功能启动器（AI 助手「我要…」直接打开功能）───
fn launcher_routes() -> Router<CmxAppState> {
    Router::new()
        // launcher/catalog 已废弃（无前端调用），见 legacy.rs
        .route("/launcher/resolve", post(launcher::launcher_resolve))
}

// ─── 注册表只读派生（DAM）+ 服务目录 + 模块清单与资源 ───
fn registry_routes() -> Router<CmxAppState> {
    Router::new()
        .route("/registry/domains", get(registry::registry_domains))
        .route("/registry/apps", get(registry::registry_apps))
        .route("/registry/modules", get(registry::registry_modules))
        .route("/registry/dam", get(registry::registry_dam))
        .route("/service-catalog", get(registry::service_catalog_list))
        .route("/service-catalog/{id}", get(registry::service_catalog_get))
        .route("/modules", get(registry::list_modules))
        .route(
            "/modules/{domain}/{application}/{module}",
            get(registry::get_module_manifest),
        )
        .route(
            "/modules/{domain}/{application}/{module}/resources/{type}",
            get(registry::get_module_resource),
        )
        .route("/module-resources", get(registry::module_resources))
}

// ─── 定义中心（DCT/DOC/BASE）───
fn definitions_routes() -> Router<CmxAppState> {
    Router::new()
        .route("/definitions/list", get(definitions::definitions_list))
        .route(
            "/definitions/config",
            get(definitions::definitions_get)
                .post(definitions::definitions_save)
                .delete(definitions::definitions_delete),
        )
        .route("/definitions/batch", post(definitions::definitions_batch))
        .route(
            "/definitions/default",
            post(definitions::definitions_set_default),
        )
}

// ─── 弹性组合 ───
fn flexible_combination_routes() -> Router<CmxAppState> {
    Router::new()
        .route(
            "/flexible-combination/list",
            get(flexible_combination::fc_list),
        )
        .route(
            "/flexible-combination/config",
            get(flexible_combination::fc_get_config)
                .post(flexible_combination::fc_save_config)
                .delete(flexible_combination::fc_delete_config),
        )
        .route(
            "/flexible-combination/resolve",
            get(flexible_combination::fc_resolve),
        )
        .route(
            "/flexible-combination/rule",
            get(flexible_combination::fc_rule),
        )
        .route(
            "/flexible-combination/validate",
            post(flexible_combination::fc_validate),
        )
        .route(
            "/flexible-combination/preview",
            post(flexible_combination::fc_preview),
        )
        .route(
            "/flexible-combination/default",
            post(flexible_combination::fc_set_default),
        )
}

// ─── 模型中心：数据库初始化 + 模块部署（真实落库）───
fn model_routes() -> Router<CmxAppState> {
    Router::new()
        .route("/model/db-state", get(model::model_db_state))
        .route("/model/init", post(model::model_init))
        .route(
            "/model/init-plan-stream",
            post(model::model_init_plan_stream),
        )
        .route("/model/init-stream", post(model::model_init_stream))
        .route("/model/deploy", post(model::model_deploy))
        .route(
            "/model/deploy-plan-stream",
            post(model::model_deploy_plan_stream),
        )
        .route("/model/deploy-stream", post(model::model_deploy_stream))
}
