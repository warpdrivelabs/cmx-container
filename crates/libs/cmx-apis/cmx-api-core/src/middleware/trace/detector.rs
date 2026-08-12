//! 内容类型检测模块。
//!
//! 提供 HTTP 请求和响应的内容类型判断，用于决定是否跳过 body 读取。

use axum::http::HeaderMap;

/// 检测是否为 multipart/form-data 请求（文件上传）。
///
/// # Arguments
///
/// * `headers` - HTTP 请求头
///
/// # Returns
///
/// 当 `Content-Type` 包含 `multipart/form-data` 时返回 `true`。
pub fn is_multipart_request(headers: &HeaderMap) -> bool {
    headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_lowercase().contains("multipart/form-data"))
        .unwrap_or(false)
}

/// 检测是否为文件下载响应。
///
/// 判断依据：
/// - `Content-Disposition` 包含 `attachment`
/// - `Content-Type` 为二进制类型（octet-stream、zip、pdf、image/* 等）
///
/// # Arguments
///
/// * `headers` - HTTP 响应头
///
/// # Returns
///
/// 当响应为文件下载或二进制内容时返回 `true`。
pub fn is_file_download_response(headers: &HeaderMap) -> bool {
    if headers
        .get("content-disposition")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_lowercase().contains("attachment"))
        .unwrap_or(false)
    {
        return true;
    }

    let content_type = match headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_lowercase())
    {
        Some(ct) => ct,
        None => return false,
    };

    const BINARY_PREFIXES: &[&str] = &[
        "application/octet-stream",
        "application/zip",
        "application/pdf",
        "application/x-rar",
        "application/x-7z",
        "application/x-tar",
        "application/gzip",
        "application/x-bzip",
        "application/x-xz",
        "image/",
        "video/",
        "audio/",
        "font/",
        "application/wasm",
    ];

    BINARY_PREFIXES
        .iter()
        .any(|prefix| content_type.starts_with(prefix))
}

/// 检测响应是否为 JSON 类型。
///
/// # Arguments
///
/// * `headers` - HTTP 响应头
///
/// # Returns
///
/// 当 `Content-Type` 包含 `application/json` 时返回 `true`。
pub fn is_json_response(headers: &HeaderMap) -> bool {
    headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_lowercase().contains("application/json"))
        .unwrap_or(false)
}
