//! REST API handler 模块
//!
//! 提供 axum 路由和 HTTP 请求处理函数。
//! 参考 Java PortalSysFileController 的接口设计，封装存储服务的文件上传、下载、
//! 删除、预签名和分片上传等 REST API。

use std::io::{Cursor, Write};
use std::sync::Arc;
use std::time::Duration;

use axum::{
    Json,
    extract::{Multipart, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

use crate::service::StorageService;
use crate::types::*;

/// 应用共享状态
///
/// 通过 `State` extractor 注入到各 handler 中，
/// 持有存储服务的动态分发引用。
#[derive(Clone)]
pub struct AppState {
    /// 存储服务实例
    pub storage_service: Arc<dyn StorageService>,
}

/// 统一 API 响应格式
///
/// 所有 REST API 均使用此结构返回 JSON 响应，
/// 包含状态码、消息和可选的数据载荷。
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ApiResponse<T: Serialize> {
    /// 业务状态码
    pub code: i32,
    /// 响应消息
    pub message: String,
    /// 响应数据载荷
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
}

impl<T: Serialize> ApiResponse<T> {
    /// 构建成功响应
    ///
    /// # Arguments
    ///
    /// * `data` - 响应数据
    ///
    /// # Returns
    ///
    /// 返回业务状态码为 200 的成功响应。
    pub fn ok(data: T) -> Self {
        Self {
            code: 200,
            message: "success".to_string(),
            data: Some(data),
        }
    }
}

/// 文件上传表单参数
///
/// 用于 utoipa 文档描述 multipart/form-data 上传请求的字段。
#[derive(Debug, Deserialize, ToSchema)]
pub struct UploadForm {
    /// 上传的文件数据（必填）
    #[schema(content_media_type = "application/octet-stream")]
    pub file: Vec<u8>,
    /// 文件关联对象类型（选填，如 avatar、attachment）
    pub object_type: Option<String>,
    /// 文件关联对象 ID（选填）
    pub object_id: Option<String>,
    /// 存储平台标识（选填，不填使用默认平台）
    pub platform: Option<String>,
}

/// 文件下载查询参数
#[derive(Debug, Deserialize, ToSchema)]
pub struct DownloadQuery {
    /// 文件唯一标识
    pub file_id: String,
    /// 是否下载缩略图（"1" 表示下载缩略图）
    pub thumbnail: Option<String>,
}

/// 文件信息查询参数
#[derive(Debug, Deserialize, ToSchema)]
pub struct FileInfoQuery {
    /// 文件唯一标识
    pub file_id: String,
}

/// 文件删除查询参数
#[derive(Debug, Deserialize, ToSchema)]
pub struct DeleteQuery {
    /// 文件唯一标识
    pub file_id: String,
}

/// 批量下载请求体
#[derive(Debug, Deserialize, ToSchema)]
pub struct BatchDownloadRequest {
    /// 待下载的文件 ID 列表
    pub file_ids: Vec<String>,
}

/// 预签名下载请求体
#[derive(Debug, Deserialize, ToSchema)]
pub struct PresignDownloadRequest {
    /// 文件唯一标识
    pub file_id: String,
    /// 预签名 URL 过期时间（秒），默认 3600
    pub expires: Option<u64>,
}

/// 预签名上传请求体
#[derive(Debug, Deserialize, ToSchema)]
pub struct PresignUploadBody {
    /// 上传文件名
    pub filename: String,
    /// 预签名 URL 过期时间（秒），默认 3600
    pub expires: Option<u64>,
    /// 文件 MIME 类型
    pub content_type: Option<String>,
    /// 存储平台标识
    pub platform: Option<String>,
}

/// 分片上传初始化请求体
#[derive(Debug, Deserialize, ToSchema)]
pub struct MultipartInitBody {
    /// 文件名
    pub filename: String,
    /// 总分片数
    pub total_parts: u32,
    /// 文件 MIME 类型
    pub content_type: Option<String>,
    /// 关联对象类型
    pub object_type: Option<String>,
    /// 关联对象 ID
    pub object_id: Option<String>,
    /// 存储平台标识
    pub platform: Option<String>,
}

/// 分片上传回调请求体
#[derive(Debug, Deserialize, ToSchema)]
pub struct MultipartPartBody {
    /// 分片上传会话 ID
    pub upload_id: String,
    /// 分片编号（从 1 开始）
    pub part_number: u32,
    /// 分片 ETag 标识
    pub e_tag: String,
    /// 分片数据大小（字节）
    pub part_size: i64,
}

/// 分片上传完成请求体
#[derive(Debug, Deserialize, ToSchema)]
pub struct MultipartCompleteBody {
    /// 分片上传会话 ID
    pub upload_id: String,
}

/// 分片上传取消请求体
#[derive(Debug, Deserialize, ToSchema)]
pub struct MultipartAbortBody {
    /// 分片上传会话 ID
    pub upload_id: String,
}

// /// 创建存储模块的 axum 路由
// ///
// /// 注册所有文件存储相关的 REST API 端点。
// ///
// /// # Arguments
// ///
// /// * `state` - 应用共享状态
// ///
// /// # Returns
// ///
// /// 返回配置好的 axum Router。
// pub fn create_router(state: AppState) -> Router {
//     Router::new()
//         .route("/api/storage/upload", post(upload_handler))
//         .route("/api/storage/download", get(download_handler))
//         .route("/api/storage/batch-download", post(batch_download_handler))
//         .route("/api/storage/info", get(file_info_handler))
//         .route("/api/storage/delete", delete(delete_handler))
//         .route("/api/storage/page", post(page_handler))
//         .route("/api/storage/presign-download", post(presign_download_handler))
//         .route("/api/storage/presign-upload", post(presign_upload_handler))
//         .route("/api/storage/multipart/init", post(multipart_init_handler))
//         .route("/api/storage/multipart/part", post(multipart_part_handler))
//         .route("/api/storage/multipart/complete", post(multipart_complete_handler))
//         .route("/api/storage/multipart/abort", post(multipart_abort_handler))
//         .with_state(state)
// }

/// 上传文件 handler
///
/// 从 multipart 表单中提取文件数据和所有元信息并调用存储服务完成上传。
///
/// # Arguments
///
/// * `state` - 应用共享状态
/// * `multipart` - multipart 表单数据，包含：
///   - `file` - 文件二进制数据（必需）
///   - `object_type` - 文件关联对象类型（可选）
///   - `object_id` - 文件关联对象 ID（可选）
///   - `platform` - 存储平台标识（可选）
///
/// # Returns
///
/// 成功时返回文件信息的 JSON 响应。
///
/// # Errors
///
/// 当未找到上传文件字段或存储服务上传失败时返回错误响应。
#[utoipa::path(
    post,
    path = "/api/storage/upload",
    tag = "文件存储",
    request_body(content = UploadForm, description = "文件上传表单", content_type = "multipart/form-data"
    ),
    responses(
        (status = 200, description = "上传成功", body = ApiResponse<FileInfo>),
        (status = 400, description = "请求错误"),
        (status = 500, description = "服务器错误")
    )
)]
pub async fn upload_handler(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    let mut file_data: Option<Bytes> = None;
    let mut original_filename: Option<String> = None;
    let mut object_type: Option<String> = None;
    let mut object_id: Option<String> = None;
    let mut platform: Option<String> = None;

    while let Some(field) = multipart.next_field().await.unwrap_or(None) {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "file" => {
                original_filename = field.file_name().map(|s| s.to_string());
                match field.bytes().await {
                    Ok(bytes) => file_data = Some(bytes),
                    Err(e) => {
                        return error_response(StatusCode::BAD_REQUEST, &format!("读取文件失败: {}", e));
                    }
                }
            }
            "object_type" => {
                if let Ok(text) = field.text().await {
                    object_type = Some(text);
                }
            }
            "object_id" => {
                if let Ok(text) = field.text().await {
                    object_id = Some(text);
                }
            }
            "platform" => {
                if let Ok(text) = field.text().await {
                    platform = Some(text);
                }
            }
            _ => {}
        }
    }

    let data = match file_data {
        Some(d) => d,
        None => return error_response(StatusCode::BAD_REQUEST, "未找到上传文件"),
    };

    let request = UploadRequest {
        data,
        original_filename,
        content_type: None,
        object_id,
        object_type,
        platform,
        user_metadata: None,
        acl: None,
    };

    match state.storage_service.upload(request).await {
        Ok(file_info) => Json(ApiResponse::ok(file_info)).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

/// 下载文件 handler
///
/// 根据文件 ID 下载文件或缩略图，返回二进制流。
///
/// # Arguments
///
/// * `state` - 应用共享状态
/// * `query` - 下载查询参数
///
/// # Returns
///
/// 成功时返回文件二进制流，失败时返回 JSON 错误响应。
///
/// # Errors
///
/// 当文件不存在或下载失败时返回错误响应。
#[utoipa::path(
    get,
    path = "/api/storage/download",
    tag = "文件存储",
    params(
        ("file_id" = String, Path, description = "文件唯一标识"),
        ("thumbnail" = Option<String>, Query, description = "是否下载缩略图（1 表示下载缩略图）", nullable = true)
    ),
    responses(
        (status = 200, description = "下载成功", content_type = "application/octet-stream"),
        (status = 404, description = "文件不存在"),
        (status = 500, description = "服务器错误")
    )
)]
pub async fn download_handler(
    State(state): State<AppState>,
    Query(query): Query<DownloadQuery>,
) -> impl IntoResponse {
    let result = if query.thumbnail.as_deref() == Some("1") {
        state.storage_service.download_thumbnail(&query.file_id).await
    } else {
        state.storage_service.download(&query.file_id).await
    };

    match result {
        Ok(download) => {
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, &download.content_type)
                .header(header::CONTENT_DISPOSITION, &download.content_disposition)
                .header(header::CONTENT_LENGTH, download.content_length)
                .body(download.data.into())
                .unwrap()
        }
        Err(e) => error_response(StatusCode::NOT_FOUND, &e.to_string()),
    }
}

/// 批量下载文件 handler
///
/// 将多个文件打包为 ZIP 并返回。
///
/// # Arguments
///
/// * `state` - 应用共享状态
/// * `body` - 批量下载请求体
///
/// # Returns
///
/// 成功时返回 ZIP 文件二进制流。
///
/// # Errors
///
/// 当文件列表为空或 ZIP 打包失败时返回错误响应。
#[utoipa::path(
    post,
    path = "/api/storage/batch-download",
    tag = "文件存储",
    request_body = BatchDownloadRequest,
    responses(
        (status = 200, description = "ZIP 包下载成功", content_type = "application/zip"),
        (status = 400, description = "请求错误"),
        (status = 404, description = "没有可下载的文件"),
        (status = 500, description = "服务器错误")
    )
)]
pub async fn batch_download_handler(
    State(state): State<AppState>,
    Json(body): Json<BatchDownloadRequest>,
) -> impl IntoResponse {
    if body.file_ids.is_empty() {
        return error_response(StatusCode::BAD_REQUEST, "文件ID列表为空");
    }

    let mut file_data_list: Vec<(String, Bytes, String)> = Vec::new();
    for file_id in &body.file_ids {
        match state.storage_service.download(file_id).await {
            Ok(download) => {
                let filename = download.file_info.original_filename
                    .unwrap_or_else(|| download.file_info.filename.clone());
                file_data_list.push((filename, download.data, download.content_type));
            }
            Err(e) => {
                tracing::warn!(file_id = %file_id, error = %e, "批量下载中跳过无法获取的文件");
            }
        }
    }

    if file_data_list.is_empty() {
        return error_response(StatusCode::NOT_FOUND, "没有可下载的文件");
    }

    let mut buffer = Vec::new();
    {
        let cursor = Cursor::new(&mut buffer);
        let mut zip = ZipWriter::new(cursor);

        let options = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);

        for (filename, data, _content_type) in &file_data_list {
            let name = sanitize_filename(filename);
            if let Err(e) = zip.start_file(name, options) {
                tracing::error!(error = %e, "创建ZIP文件失败");
                return error_response(StatusCode::INTERNAL_SERVER_ERROR, &format!("创建ZIP文件失败: {}", e));
            }
            if let Err(e) = zip.write_all(data) {
                tracing::error!(error = %e, "写入ZIP数据失败");
                return error_response(StatusCode::INTERNAL_SERVER_ERROR, &format!("写入ZIP数据失败: {}", e));
            }
        }

        if let Err(e) = zip.finish() {
            tracing::error!(error = %e, "完成ZIP文件失败");
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, &format!("完成ZIP文件失败: {}", e));
        }
    }

    let total_size: usize = file_data_list.iter().map(|(_, d, _)| d.len()).sum();
    tracing::info!(file_count = file_data_list.len(), total_bytes = total_size, "批量下载ZIP打包完成");

    let filename = format!("files_{}.zip", chrono::Local::now().format("%Y%m%d%H%M%S"));
    let disposition = format!(
        "attachment; filename=\"{}\"; filename*=UTF-8''{}",
        filename, urlencoding::encode(&filename)
    );

    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/zip"),
            (header::CONTENT_DISPOSITION, &disposition),
            (header::CONTENT_LENGTH, buffer.len().to_string().as_str()),
        ],
        buffer,
    ).into_response()
}

/// 清理文件名，移除路径分隔符
///
/// # Arguments
///
/// * `filename` - 原始文件名
///
/// # Returns
///
/// 清理后的安全文件名。
fn sanitize_filename(filename: &str) -> String {
    filename
        .replace(['/', '\\', ':'], "_")
}

/// 获取文件信息 handler
///
/// 根据文件 ID 查询并返回文件的元数据信息。
///
/// # Arguments
///
/// * `state` - 应用共享状态
/// * `query` - 文件信息查询参数
///
/// # Returns
///
/// 成功时返回文件信息的 JSON 响应。
///
/// # Errors
///
/// 当文件不存在时返回 404 错误响应。
#[utoipa::path(
    get,
    path = "/api/storage/info",
    tag = "文件存储",
    params(
        ("file_id" = String, Query, description = "文件唯一标识")
    ),
    responses(
        (status = 200, description = "获取成功", body = ApiResponse<FileInfo>),
        (status = 404, description = "文件不存在"),
        (status = 500, description = "服务器错误")
    )
)]
pub async fn file_info_handler(
    State(state): State<AppState>,
    Query(query): Query<FileInfoQuery>,
) -> impl IntoResponse {
    match state.storage_service.get_file_info(&query.file_id).await {
        Ok(file_info) => Json(ApiResponse::ok(file_info)).into_response(),
        Err(e) => error_response(StatusCode::NOT_FOUND, &e.to_string()),
    }
}

/// 删除文件 handler
///
/// 根据文件 ID 删除指定的文件。
///
/// # Arguments
///
/// * `state` - 应用共享状态
/// * `query` - 删除查询参数
///
/// # Returns
///
/// 成功时返回空 JSON 响应。
///
/// # Errors
///
/// 当文件删除失败时返回 500 错误响应。
#[utoipa::path(
    delete,
    path = "/api/storage/delete",
    tag = "文件存储",
    params(
        ("file_id" = String, Query, description = "文件唯一标识")
    ),
    responses(
        (status = 200, description = "删除成功"),
        (status = 500, description = "服务器错误")
    )
)]
pub async fn delete_handler(
    State(state): State<AppState>,
    Query(query): Query<DeleteQuery>,
) -> impl IntoResponse {
    match state.storage_service.delete(&query.file_id).await {
        Ok(()) => Json(ApiResponse::<()>::ok(())).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

/// 分页查询文件列表 handler
///
/// 根据查询条件分页检索文件信息列表。
///
/// # Arguments
///
/// * `state` - 应用共享状态
/// * `query` - 文件查询条件
///
/// # Returns
///
/// 成功时返回分页结果的 JSON 响应。
///
/// # Errors
///
/// 当查询失败时返回 500 错误响应。
#[utoipa::path(
    post,
    path = "/api/storage/page",
    tag = "文件存储",
    request_body = FileQuery,
    responses(
        (status = 200, description = "查询成功", body = ApiResponse<FilePage>),
        (status = 500, description = "服务器错误")
    )
)]
pub async fn page_handler(
    State(state): State<AppState>,
    Json(query): Json<FileQuery>,
) -> impl IntoResponse {
    match state.storage_service.list_files(query).await {
        Ok(page) => Json(ApiResponse::ok(page)).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

/// 预签名下载 handler
///
/// 生成文件的预签名下载 URL。
///
/// # Arguments
///
/// * `state` - 应用共享状态
/// * `body` - 预签名下载请求体
///
/// # Returns
///
/// 成功时返回预签名 URL 的 JSON 响应。
///
/// # Errors
///
/// 当预签名生成失败时返回 500 错误响应。
#[utoipa::path(
    post,
    path = "/api/storage/presign-download",
    tag = "文件存储",
    request_body = PresignDownloadRequest,
    responses(
        (status = 200, description = "生成成功", body = ApiResponse<String>),
        (status = 500, description = "服务器错误")
    )
)]
pub async fn presign_download_handler(
    State(state): State<AppState>,
    Json(body): Json<PresignDownloadRequest>,
) -> impl IntoResponse {
    let expires = Duration::from_secs(body.expires.unwrap_or(3600));
    match state.storage_service.presign_download(&body.file_id, expires).await {
        Ok(url) => Json(ApiResponse::ok(url)).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

/// 预签名上传 handler
///
/// 生成文件的预签名上传 URL。
///
/// # Arguments
///
/// * `state` - 应用共享状态
/// * `body` - 预签名上传请求体
///
/// # Returns
///
/// 成功时返回预签名 URL 和文件 ID 的 JSON 响应。
///
/// # Errors
///
/// 当预签名生成失败时返回 500 错误响应。
#[utoipa::path(
    post,
    path = "/api/storage/presign-upload",
    tag = "文件存储",
    request_body = PresignUploadBody,
    responses(
        (status = 200, description = "生成成功", body = ApiResponse<PresignUploadResult>),
        (status = 500, description = "服务器错误")
    )
)]
pub async fn presign_upload_handler(
    State(state): State<AppState>,
    Json(body): Json<PresignUploadBody>,
) -> impl IntoResponse {
    let expires = Duration::from_secs(body.expires.unwrap_or(3600));
    let request = PresignUploadRequest {
        filename: body.filename,
        content_type: body.content_type,
        platform: body.platform,
    };
    match state.storage_service.presign_upload(request, expires).await {
        Ok(result) => Json(ApiResponse::ok(result)).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

/// 分片上传初始化 handler
///
/// 创建分片上传会话，返回各分片的预签名上传 URL。
///
/// # Arguments
///
/// * `state` - 应用共享状态
/// * `body` - 分片上传初始化请求体
///
/// # Returns
///
/// 成功时返回分片上传会话信息的 JSON 响应。
///
/// # Errors
///
/// 当初始化失败时返回 500 错误响应。
#[utoipa::path(
    post,
    path = "/api/storage/multipart/init",
    tag = "文件存储",
    request_body = MultipartInitBody,
    responses(
        (status = 200, description = "初始化成功", body = ApiResponse<MultipartSession>),
        (status = 500, description = "服务器错误")
    )
)]
pub async fn multipart_init_handler(
    State(state): State<AppState>,
    Json(body): Json<MultipartInitBody>,
) -> impl IntoResponse {
    let request = MultipartInitRequest {
        filename: body.filename,
        total_parts: body.total_parts,
        content_type: body.content_type,
        object_type: body.object_type,
        object_id: body.object_id,
        platform: body.platform,
    };
    match state.storage_service.init_multipart_upload(request).await {
        Ok(session) => Json(ApiResponse::ok(session)).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

/// 分片上传回调 handler
///
/// 记录单个分片上传完成后的 ETag 和大小信息。
///
/// # Arguments
///
/// * `state` - 应用共享状态
/// * `body` - 分片回调请求体
///
/// # Returns
///
/// 成功时返回分片信息的 JSON 响应。
///
/// # Errors
///
/// 当回调处理失败时返回 500 错误响应。
#[utoipa::path(
    post,
    path = "/api/storage/multipart/part",
    tag = "文件存储",
    request_body = MultipartPartBody,
    responses(
        (status = 200, description = "记录成功", body = ApiResponse<PartInfo>),
        (status = 500, description = "服务器错误")
    )
)]
pub async fn multipart_part_handler(
    State(state): State<AppState>,
    Json(body): Json<MultipartPartBody>,
) -> impl IntoResponse {
    let part = PartData {
        upload_id: body.upload_id.clone(),
        part_number: body.part_number,
        e_tag: body.e_tag,
        part_size: body.part_size,
    };
    match state.storage_service.upload_part(&body.upload_id, part).await {
        Ok(info) => Json(ApiResponse::ok(info)).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

/// 完成分片上传 handler
///
/// 合并所有已上传的分片，完成文件上传流程。
///
/// # Arguments
///
/// * `state` - 应用共享状态
/// * `body` - 分片完成请求体
///
/// # Returns
///
/// 成功时返回文件信息的 JSON 响应。
///
/// # Errors
///
/// 当合并失败时返回 500 错误响应。
#[utoipa::path(
    post,
    path = "/api/storage/multipart/complete",
    tag = "文件存储",
    request_body = MultipartCompleteBody,
    responses(
        (status = 200, description = "完成成功", body = ApiResponse<FileInfo>),
        (status = 500, description = "服务器错误")
    )
)]
pub async fn multipart_complete_handler(
    State(state): State<AppState>,
    Json(body): Json<MultipartCompleteBody>,
) -> impl IntoResponse {
    match state.storage_service.complete_multipart_upload(&body.upload_id).await {
        Ok(file_info) => Json(ApiResponse::ok(file_info)).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

/// 取消分片上传 handler
///
/// 取消指定的分片上传会话，清理已上传的分片数据。
///
/// # Arguments
///
/// * `state` - 应用共享状态
/// * `body` - 分片取消请求体
///
/// # Returns
///
/// 成功时返回空 JSON 响应。
///
/// # Errors
///
/// 当取消操作失败时返回 500 错误响应。
#[utoipa::path(
    post,
    path = "/api/storage/multipart/abort",
    tag = "文件存储",
    request_body = MultipartAbortBody,
    responses(
        (status = 200, description = "取消成功"),
        (status = 500, description = "服务器错误")
    )
)]
pub async fn multipart_abort_handler(
    State(state): State<AppState>,
    Json(body): Json<MultipartAbortBody>,
) -> impl IntoResponse {
    match state.storage_service.abort_multipart_upload(&body.upload_id).await {
        Ok(()) => Json(ApiResponse::<()>::ok(())).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

/// 构建错误响应
///
/// 将 HTTP 状态码和错误消息封装为统一 JSON 响应格式。
///
/// # Arguments
///
/// * `status` - HTTP 状态码
/// * `message` - 错误消息
///
/// # Returns
///
/// JSON 格式的错误响应。
fn error_response(status: StatusCode, message: &str) -> Response {
    let body = ApiResponse::<String> {
        code: status.as_u16() as i32,
        message: message.to_string(),
        data: None,
    };
    (status, Json(body)).into_response()
}
