//! cmx-storage-api —— 文件存储模块的 HTTP 层。
//!
//! 薄 ModuleRoutes 胶水：把 cmx-storage::handler 的 HTTP 函数装配成 axum Router。
//! HTTP 函数本就在 cmx-storage，本 crate 只做路由注册。
//!
//! `FromRef<CmxAppState> for cmx_storage::handler::AppState` 由 cmx-api-core 实现
//! （孤儿规则要求与 CmxAppState 同 crate），故本 crate 无需也不能再 impl。
//!
//! StorageApiDoc 提供本域 OpenApi 切片（`cmx_storage::handler::*` 的 `#[utoipa::path]`），
//! 由 platform-app 用 `OpenApi::merge()` 聚合。

pub mod openapi;

pub use openapi::StorageApiDoc;

use axum::Router;
use axum::routing::{get, post};

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
            // 简单上传（单次提交，适合小文件）
            .route("/storage/upload", post(upload_handler))
            // 下载单个文件
            .route("/storage/download", get(download_handler))
            // 批量打包下载（多文件合并为 zip）
            .route("/storage/batch-download", post(batch_download_handler))
            // 查询文件元信息（大小 / mime / 摘要等，不返回内容）
            .route("/storage/info", get(file_info_handler))
            // 删除文件
            .route("/storage/delete", post(delete_handler))  // 既有接口，已按新规范改用 POST
            // 文件分页查询（按 owner / 业务维度筛选）
            .route("/storage/page", post(page_handler))
            // 预签名下载 URL（前端直链下载，绕过后端流量）
            .route("/storage/presign-download", post(presign_download_handler))
            // 预签名上传 URL（前端直传对象存储）
            .route("/storage/presign-upload", post(presign_upload_handler))
            // 分片上传：初始化（返回 upload_id）
            .route("/storage/multipart/init", post(multipart_init_handler))
            // 分片上传：上传单个分片
            .route("/storage/multipart/part", post(multipart_part_handler))
            // 分片上传：合并所有分片完成上传
            .route(
                "/storage/multipart/complete",
                post(multipart_complete_handler),
            )
            // 分片上传：中止并清理已上传分片
            .route("/storage/multipart/abort", post(multipart_abort_handler))
    }

    fn prefix() -> &'static str {
        "storage"
    }

    fn module_name(&self) -> &'static str {
        "storage"
    }
}
