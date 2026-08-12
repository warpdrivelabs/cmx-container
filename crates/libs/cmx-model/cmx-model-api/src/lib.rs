//! 模型中心 HTTP 层（定义中心 + 弹性组合 + 数据库部署）。
//!
//! 对标 cmx-doc-api / cmx-dct-api 的分层模式：
//! - cmx-model-meta：元数据定义读写（JSON 存储）
//! - cmx-model-deploy：编译/初始化/部署（DB 落库）
//! - cmx-model-api（本 crate）：薄 axum handler + ModuleRoutes 路由聚合

pub mod handlers;

use axum::Router;
use axum::routing::{get, post};
use cmx_api_core::app_state::CmxAppState;
use cmx_api_core::routes::traits::ModuleRoutes;

/// 模型中心路由模块。
pub struct ModelModule;

impl ModuleRoutes for ModelModule {
    fn routes(self) -> Router<CmxAppState> {
        Router::new()
            .merge(definitions_routes())
            .merge(flexible_combination_routes())
            .merge(deploy_routes())
    }

    fn prefix() -> &'static str {
        "model"
    }

    fn module_name(&self) -> &'static str {
        "model"
    }
}

// ─── 定义中心（DCT/DOC/BASE）───
fn definitions_routes() -> Router<CmxAppState> {
    Router::new()
        .route(
            "/definitions/list",
            get(handlers::definitions::definitions_list),
        )
        .route(
            "/definitions/config",
            get(handlers::definitions::definitions_get)
                .post(handlers::definitions::definitions_save)
                .delete(handlers::definitions::definitions_delete),
        )
        .route(
            "/definitions/batch",
            post(handlers::definitions::definitions_batch),
        )
        .route(
            "/definitions/default",
            post(handlers::definitions::definitions_set_default),
        )
}

// ─── 弹性组合 ───
fn flexible_combination_routes() -> Router<CmxAppState> {
    Router::new()
        .route(
            "/flexible-combination/list",
            get(handlers::flexible_combination::fc_list),
        )
        .route(
            "/flexible-combination/config",
            get(handlers::flexible_combination::fc_get_config)
                .post(handlers::flexible_combination::fc_save_config)
                .delete(handlers::flexible_combination::fc_delete_config),
        )
        .route(
            "/flexible-combination/resolve",
            get(handlers::flexible_combination::fc_resolve),
        )
        .route(
            "/flexible-combination/rule",
            get(handlers::flexible_combination::fc_rule),
        )
        .route(
            "/flexible-combination/validate",
            post(handlers::flexible_combination::fc_validate),
        )
        .route(
            "/flexible-combination/preview",
            post(handlers::flexible_combination::fc_preview),
        )
        .route(
            "/flexible-combination/default",
            post(handlers::flexible_combination::fc_set_default),
        )
}

// ─── 数据库初始化 + 模块部署（真实落库）───
fn deploy_routes() -> Router<CmxAppState> {
    Router::new()
        .route("/model/db-state", get(handlers::deploy::model_db_state))
        .route("/model/init", post(handlers::deploy::model_init))
        .route(
            "/model/init-plan-stream",
            post(handlers::deploy::model_init_plan_stream),
        )
        .route("/model/init-stream", post(handlers::deploy::model_init_stream))
        .route("/model/deploy", post(handlers::deploy::model_deploy))
        .route(
            "/model/deploy-plan-stream",
            post(handlers::deploy::model_deploy_plan_stream),
        )
        .route("/model/deploy-stream", post(handlers::deploy::model_deploy_stream))
}
