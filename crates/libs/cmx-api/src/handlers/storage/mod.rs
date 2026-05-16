//! Storage 文件存储路由模块
//!
//! 将 cmx-storage 的 REST API 路由集成到主应用的 CmxAppState 中。
//! 通过 `FromRef<CmxAppState>` for `cmx_storage::handler::AppState` 实现
//! 自动状态提取，无需手动转换。

use crate::app_state::CmxAppState;
use crate::routes::traits::ModuleRoutes;
use axum::Router;
use axum::extract::FromRef;
use axum::routing::{delete, get, post};

use cmx_storage::handler::{
    batch_download_handler, delete_handler, download_handler, file_info_handler,
    multipart_abort_handler, multipart_complete_handler, multipart_init_handler,
    multipart_part_handler, page_handler, presign_download_handler, presign_upload_handler,
    upload_handler, AppState,
};

impl FromRef<CmxAppState> for AppState {
    fn from_ref(state: &CmxAppState) -> Self {
        let storage_service = state
            .storage_service()
            .expect("storage_service 未初始化");
        Self {
            storage_service: storage_service.clone(),
        }
    }
}

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
            .route("/storage/multipart/complete", post(multipart_complete_handler))
            .route("/storage/multipart/abort", post(multipart_abort_handler))
    }

    fn prefix() -> &'static str {
        "storage"
    }

    fn module_name(&self) -> &'static str {
        "storage"
    }
}
