//! 请求追踪中间件核心实现。
//!
//! 提供双模式请求追踪：INFO 模式零开销记录摘要，DEBUG 模式记录完整请求/响应详情。

use axum::{
    body::Body,
    body::HttpBody,
    extract::Request,
    http::Method,
    middleware::Next,
    response::Response,
};
use std::sync::OnceLock;
use std::time::Instant;
use tracing::{debug, info, warn};

use super::config::TraceConfig;
use super::detector::{is_file_download_response, is_json_response, is_multipart_request};
use super::sanitizer::{collect_headers, sanitize_body, sanitize_json_value};

/// 请求追踪中间件入口。
///
/// 根据运行时日志级别自动选择追踪模式：
/// - DEBUG/TRACE 级别：记录完整请求头、请求体、响应体（脱敏）
/// - INFO 及以上级别：仅记录方法、路径、查询参数、状态码、耗时
///
/// # Arguments
///
/// * `req` - 入站 HTTP 请求
/// * `next` - 中间件链中的下一个 handler
///
/// # Returns
///
/// 下游 handler 的响应，追踪信息通过 `tracing` 日志输出。
pub async fn trace_layer(req: Request<Body>, next: Next) -> Response {
    let start = Instant::now();
    let method = req.method().clone();
    let uri = req.uri().clone();
    let path = uri.path().to_string();
    let query = uri.query().map(|s| s.to_string());

    if is_debug_enabled() {
        trace_verbose(req, next, start, method, path, query).await
    } else {
        trace_lightweight(req, next, start, method, path, query).await
    }
}

/// 轻量模式追踪，仅记录请求摘要。
///
/// 不读取请求体和响应体，不解析 JSON，不做脱敏处理，
/// 仅输出方法、路径、查询参数、状态码和耗时。
///
/// # Arguments
///
/// * `req` - 入站 HTTP 请求
/// * `next` - 中间件链中的下一个 handler
/// * `start` - 请求开始时间
/// * `method` - HTTP 方法
/// * `path` - 请求路径
/// * `query` - 查询参数
async fn trace_lightweight(
    req: Request<Body>,
    next: Next,
    start: Instant,
    method: Method,
    path: String,
    query: Option<String>,
) -> Response {
    let response = next.run(req).await;
    let duration = start.elapsed();
    let status = response.status();

    info!(
        target: "req_trace",
        "请求摘要: {} {} query={:?} -> {} ({:?})",
        method,
        path,
        query,
        status.as_u16(),
        duration
    );

    if status.as_u16() >= 500 {
        warn!(
            target: "req_trace",
            "服务端错误: {} {} -> {}",
            method,
            path,
            status.as_u16()
        );
    }

    response
}

/// 详细模式追踪，记录完整请求/响应详情。
///
/// 读取请求体和响应体并脱敏输出，自动排除 multipart 上传和文件下载。
///
/// # Arguments
///
/// * `req` - 入站 HTTP 请求
/// * `next` - 中间件链中的下一个 handler
/// * `start` - 请求开始时间
/// * `method` - HTTP 方法
/// * `path` - 请求路径
/// * `query` - 查询参数
async fn trace_verbose(
    req: Request<Body>,
    next: Next,
    start: Instant,
    method: Method,
    path: String,
    query: Option<String>,
) -> Response {
    let config = TraceConfig::default();
    let headers = collect_headers(req.headers());
    let headers_json = serde_json::Value::Object(
        headers
            .iter()
            .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
            .collect(),
    );

    // multipart 请求不读取 body，直接透传给 handler
    if is_multipart_request(req.headers()) {
        let response = next.run(req).await;
        let duration = start.elapsed();
        let status = response.status();

        debug!(
            target: "req_trace",
            resp_status = status.as_u16(),
            resp_duration_ms = duration.as_millis() as u64,
            "文件上传: {} {} query={:?} | {} {:?}",
            method, path, query, status.as_u16(), duration
        );

        if status.as_u16() >= 500 {
            warn!(
                target: "req_trace",
                "服务端错误: {} {} -> {}",
                method, path, status.as_u16()
            );
        }

        return response;
    }

    // 读取并脱敏请求体，重建 Request 传递给下游
    let (parts, body) = req.into_parts();
    let body_bytes = match axum::body::to_bytes(body, config.max_request_body_size).await {
        Ok(bytes) => bytes.to_vec(),
        Err(_) => Vec::new(),
    };
    let req_body_preview = sanitize_body(&body_bytes);

    let new_req = Request::from_parts(parts, Body::from(body_bytes));
    let mut response = next.run(new_req).await;
    let duration = start.elapsed();
    let status = response.status();

    // 文件下载响应不读取 body
    if is_file_download_response(response.headers()) {
        debug!(
            target: "req_trace",
            resp_status = status.as_u16(),
            resp_duration_ms = duration.as_millis() as u64,
            "文件下载: {} {} query={:?} headers={} body={} | {} {:?}",
            method, path, query, headers_json, req_body_preview, status.as_u16(), duration
        );

        if status.as_u16() >= 500 {
            warn!(
                target: "req_trace",
                "服务端错误: {} {} -> {}",
                method, path, status.as_u16()
            );
        }

        return response;
    }

    // 读取并脱敏响应体，仅处理 JSON 类型
    let resp_body_preview = extract_response_body(&mut response, &config).await;

    debug!(
        target: "req_trace",
        resp_status = status.as_u16(),
        resp_duration_ms = duration.as_millis() as u64,
        "请求详情: {} {} query={:?} headers={} body={} | 响应: {} body={} {:?}",
        method, path, query, headers_json, req_body_preview,
        status.as_u16(), resp_body_preview, duration
    );

    if status.as_u16() >= 500 {
        warn!(
            target: "req_trace",
            "服务端错误: {} {} -> {}",
            method, path, status.as_u16()
        );
    }

    response
}

/// 提取响应体预览并重建 Response。
///
/// 对 JSON 响应提取并脱敏预览，对非 JSON 响应显示大小。
/// 通过 `mem::replace` 临时取出 body，读取后将 bytes 重新封装回去。
///
/// # Arguments
///
/// * `response` - HTTP 响应，body 会被临时取出后重建
/// * `config` - 追踪配置，控制读取上限和预览截断长度
///
/// # Returns
///
/// 响应体的脱敏预览字符串。
async fn extract_response_body(response: &mut Response<Body>, config: &TraceConfig) -> String {
    if !is_json_response(response.headers()) {
        let size = response.body().size_hint().upper().unwrap_or(u64::MAX);
        return format!("<non-json, {} bytes>", size);
    }

    let body = std::mem::replace(response.body_mut(), Body::empty());

    match axum::body::to_bytes(body, config.max_response_body_size).await {
        Ok(bytes) => {
            let byte_count = bytes.len();
            let preview = if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                sanitize_json_value(&json).to_string()
            } else if let Ok(s) = std::str::from_utf8(&bytes) {
                if s.len() > 512 {
                    format!("{}... ({} chars)", &s[..512], s.len())
                } else {
                    s.to_string()
                }
            } else {
                format!("<binary: {} bytes>", byte_count)
            };

            let final_preview = if preview.len() > config.max_preview_length {
                format!(
                    "<response body too large: {} chars, {} bytes>",
                    preview.len(),
                    byte_count
                )
            } else {
                preview
            };

            *response.body_mut() = Body::from(bytes);

            final_preview
        }
        Err(_) => {
            *response.body_mut() = Body::empty();
            "<failed to read body>".to_string()
        }
    }
}

/// 检测当前日志级别是否启用了 DEBUG。
///
/// 通过 `OnceLock` 缓存 `RUST_LOG` 环境变量的解析结果，
/// 首次调用时读取并缓存，后续调用仅做一次原子读取。
fn is_debug_enabled() -> bool {
    static DEBUG_ENABLED: OnceLock<bool> = OnceLock::new();
    *DEBUG_ENABLED.get_or_init(|| {
        let rust_log = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());
        let lower = rust_log.to_lowercase();
        lower.contains("debug") || lower.contains("trace")
    })
}
