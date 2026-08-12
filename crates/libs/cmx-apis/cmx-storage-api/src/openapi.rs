//! cmx-storage-api 的 OpenApi 切片。
//!
//! 文件存储相关 paths + schemas（从 cmx-common-api/openapi.rs 迁入，原阶段 2a 遗留），
//! 由 platform-app 用 `OpenApi::merge()` 聚合。handler 函数的 `#[utoipa::path]` 在
//! cmx-storage::handler，本切片只做路径注册。

use utoipa::OpenApi;

/// 文件存储 OpenApi 切片。
#[derive(OpenApi)]
#[openapi(
    paths(
        cmx_storage::handler::upload_handler,
        cmx_storage::handler::download_handler,
        cmx_storage::handler::batch_download_handler,
        cmx_storage::handler::file_info_handler,
        cmx_storage::handler::delete_handler,
        cmx_storage::handler::page_handler,
        cmx_storage::handler::presign_download_handler,
        cmx_storage::handler::presign_upload_handler,
        cmx_storage::handler::multipart_init_handler,
        cmx_storage::handler::multipart_part_handler,
        cmx_storage::handler::multipart_complete_handler,
        cmx_storage::handler::multipart_abort_handler,
    ),
    components(
        schemas(
            cmx_storage::types::FileInfo,
            cmx_storage::types::FileQuery,
            cmx_storage::types::FilePage,
            cmx_storage::types::MultipartSession,
            cmx_storage::types::PartInfo,
            cmx_storage::types::PresignUploadResult,
            cmx_storage::types::PresignUploadRequest,
            cmx_storage::handler::ApiResp<cmx_storage::types::FileInfo>,
            cmx_storage::handler::ApiResp<cmx_storage::types::FilePage>,
            cmx_storage::handler::ApiResp<cmx_storage::types::MultipartSession>,
            cmx_storage::handler::ApiResp<cmx_storage::types::PartInfo>,
            cmx_storage::handler::ApiResp<cmx_storage::types::PresignUploadResult>,
            cmx_storage::handler::ApiResp<String>,
            cmx_storage::handler::DownloadQuery,
            cmx_storage::handler::FileInfoQuery,
            cmx_storage::handler::DeleteQuery,
            cmx_storage::handler::BatchDownloadRequest,
            cmx_storage::handler::PresignDownloadRequest,
            cmx_storage::handler::PresignUploadBody,
            cmx_storage::handler::MultipartInitBody,
            cmx_storage::handler::MultipartPartBody,
            cmx_storage::handler::MultipartCompleteBody,
            cmx_storage::handler::MultipartAbortBody,
        )
    )
)]
pub struct StorageApiDoc;
