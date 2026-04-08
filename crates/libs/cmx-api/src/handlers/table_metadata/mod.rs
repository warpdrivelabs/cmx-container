//! 表元数据查询 Handler
//!
//! 提供 cmx_meta_table_define 表的列表和分页查询接口

pub mod handler;

use crate::app_state::CmxAppState;
use crate::routes::traits::ModuleRoutes;
use axum::routing::{get, post};
use axum::Router;

/// TableMetadata 模块路由
pub struct TableMetadataModule;

impl ModuleRoutes for TableMetadataModule {
    fn routes(self) -> Router<CmxAppState> {
        Router::new()
            .nest("/table-metadata", inner_routes())
    }

    fn prefix() -> &'static str {
        "table-metadata"
    }

    fn module_name(&self) -> &'static str {
        "table_metadata"
    }
}

fn inner_routes() -> Router<CmxAppState> {
    Router::new()
        .route("/get", get(handler::table_metadata_get_by_id))
        .route("/list", post(handler::table_metadata_list))
        .route("/page", post(handler::table_metadata_page))
}
