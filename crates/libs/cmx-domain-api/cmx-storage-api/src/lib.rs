//! cmx-storage-api —— 文件存储模块的 HTTP 层。
//!
//! 薄 ModuleRoutes 胶水：把 cmx-storage::handler 的 HTTP 函数装配成 axum Router。
//! HTTP 函数本就在 cmx-storage，本 crate 只做路由注册。
//!
//! `FromRef<CmxAppState> for cmx_storage::handler::AppState` 由 cmx-api-core 实现
//! （孤儿规则要求与 CmxAppState 同 crate），故本 crate 无需也不能再 impl。
//!
//! Swagger 路径（`cmx_storage::handler::*` 的 `#[utoipa::path]`）暂仍由 cmx-api 的 ApiDoc
//! 聚合（路径引用 cmx_storage，与路由位置无关），阶段 3 统一整理时随 OpenApi 合并上移。

use axum::Router;
use axum::routing::{delete, get, post};

use cmx_api_core::CmxAppState;
use cmx_api_core::ModuleRoutes;

use cmx_storage::handler::{
    batch_download_handler, delete_handler, download_handler, file_info_handler,
    multipart_abort_handler, multipart_complete_handler, multipart_init_handler,
    multipart_part_handler, page_handler, presign_download_handler, presign_upload_handler,
    upload_handler,
};

/// 文件存储路由聚合（实现 cmx-api-core 的 ModuleRoutes，由 cmx-platform-app 合并进主路由）。
pub struct StorageModule;

impl ModuleRoutes for StorageModule {
    fn routes(self) -> Router<CmxAppState> {
        Router::new()
            .route("/storage/upload", post(upload_handler))
            .route("/storage/download", get(download_handler))
            .route("/storage/batch-download", post(batch_download_handler))
            .route("/storage/info", get(file_info_handler))
            .route("/storage/delete", delete(delete_handler))
            .route("/storage/page", post(page_handler))
            .route("/storage/presign-download", post(presign_download_handler))
            .route("/storage/presign-upload", post(presign_upload_handler))
            .route("/storage/multipart/init", post(multipart_init_handler))
            .route("/storage/multipart/part", post(multipart_part_handler))
            .route(
                "/storage/multipart/complete",
                post(multipart_complete_handler),
            )
            .route("/storage/multipart/abort", post(multipart_abort_handler))
    }

    fn prefix() -> &'static str {
        "storage"
    }

    fn module_name(&self) -> &'static str {
        "storage"
    }
}
