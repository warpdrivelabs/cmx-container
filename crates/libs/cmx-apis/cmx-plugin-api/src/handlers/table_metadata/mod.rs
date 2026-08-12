//! 表元数据查询 Handler
//!
//! 提供 cmx_meta_table_define 表的列表和分页查询接口

pub mod handler;

use cmx_api_core::CmxAppState;
use cmx_api_core::ModuleRoutes;
use axum::Router;
use axum::routing::{get, post};

/// TableMetadata 模块路由
pub struct TableMetadataModule;

impl ModuleRoutes for TableMetadataModule {
    fn routes(self) -> Router<CmxAppState> {
        Router::new().nest("/table-metadata", inner_routes())
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
        // 按主键 id 查询表元数据定义
        .route("/get", get(handler::table_metadata_get_by_id))
        // 按表名查询表元数据定义
        .route("/get-by-name", get(handler::table_metadata_get_by_name))
        // 判断指定表名的元数据是否已登记
        .route("/exists", get(handler::table_metadata_exists))
        // 列表查询表元数据（按条件）
        .route("/list", post(handler::table_metadata_list))
        // 分页查询表元数据
        .route("/page", post(handler::table_metadata_page))
}
