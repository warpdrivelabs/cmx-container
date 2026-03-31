//! 请求追踪中间件
//!
//! 打印请求参数、请求头和响应结果，用于调试和日志追踪。

use axum::{
    body::Body,
    extract::Request,
    middleware::Next,
    response::Response,
};
use axum::body::HttpBody;
use std::time::Instant;
use tracing::{debug, info, warn};

/// 请求追踪中间件
///
/// 记录以下信息：
/// - 请求方法、路径、查询参数
/// - 请求头（排除敏感字段）
/// - 请求体（排除文件和二进制字段）
/// - 响应状态码、处理耗时、响应体（仅 JSON）
///
/// 排除的请求头：Authorization、Cookie、Set-Cookie、Sec-WebSocket-Key 等
/// 排除的请求字段：password、secret、token、file、binary 等（不区分大小写）
///
/// 注意：multipart（文件上传）请求会被透传，不读取 body，以确保 handler 能正常获取文件流。
pub async fn mw_trace(req: Request<Body>, next: Next) -> Response {
    let start = Instant::now();
    let method = req.method().clone();
    let uri = req.uri().clone();
    let path = uri.path().to_string();
    let query = uri.query().map(|s| s.to_string());

    let headers = collect_headers(req.headers());

    let content_type = req
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_lowercase());

    let is_multipart = content_type
        .as_ref()
        .map(|ct| ct.contains("multipart/form-data"))
        .unwrap_or(false);

    if is_multipart {
        let mut response = next.run(req).await;
        let duration = start.elapsed();
        let status = response.status();
        let resp_body_preview = extract_and_log_response_body(&mut response).await;

        info!(
            target: "req_trace",
            "━━━━━━━━━━━━━━━━━━━━req trace print━━━━━━━━━━━━━━━━━━━━━━\n\
             --REQUEST [MULTIPART - BODY SKIPPED]\n\
             ┣ path: {} {}\n\
             ┣ query: {:?}\n\
             ┣ headers: {:?}\n\
             ┗ body: <multipart/form-data - skipped>\n\
             --RESPONSE\n\
             ┣ status: {}\n\
             ┣ body: {}\n\
             ┗ duration: {:?}",
            method,
            path,
            query,
            headers,
            status.as_u16(),
            resp_body_preview,
            duration,
        );

        return response;
    }

    let (parts, body) = req.into_parts();
    let body_bytes = match axum::body::to_bytes(body, 10 * 1024 * 1024).await {
        Ok(bytes) => bytes.to_vec(),
        Err(_) => Vec::new(),
    };
    let req_body_preview = sanitize_body(&body_bytes);

    let new_req = Request::from_parts(parts, Body::from(body_bytes.clone()));
    let mut response = next.run(new_req).await;
    let duration = start.elapsed();

    let status = response.status();
    let resp_body_preview = extract_and_log_response_body(&mut response).await;

    info!(
        target: "req_trace",
        "━━━━━━━━━━━━━━━━━━━━req trace print━━━━━━━━━━━━━━━━━━━━━━\n\
         --REQUEST\n\
         ┣ path: {} {}\n\
         ┣ query: {:?}\n\
         ┣ headers: {:?}\n\
         ┗ body: {}\n\
         --RESPONSE\n\
         ┣ status: {}\n\
         ┣ body: {}\n\
         ┗ duration: {:?}",
        method,
        path,
        query,
        headers,
        req_body_preview,
        status.as_u16(),
        resp_body_preview,
        duration,
    );

    if status.as_u16() >= 500 {
        warn!(
            target: "req_trace",
            "⚠️  SERVER ERROR - {} {} - {}",
            method,
            path,
            status.as_u16()
        );
    }
    
    response
}

/// 收集请求头，排除敏感字段
fn collect_headers(headers: &axum::http::HeaderMap) -> Vec<(String, String)> {
    const SENSITIVE_HEADERS: &[&str] = &[
        "authorization",
        "cookie",
        "set-cookie",
        "sec-websocket-key",
        "x-api-key",
        "x-auth-token",
        "proxy-authorization",
        "connection",
        "accept-language",
        "user-agent",
        "host",
        "cache-control",
        "pragma",
        "accept",
        "accept-encoding",
    ];

    headers
        .iter()
        .filter(|(name, _)| {
            let name_lower = name.as_str().to_lowercase();
            !SENSITIVE_HEADERS.contains(&name_lower.as_str())
        })
        .map(|(name, value)| {
            (
                name.as_str().to_string(),
                value.to_str().unwrap_or("<binary>").to_string(),
            )
        })
        .collect()
}

/// 清理请求体，排除敏感字段和二进制数据
fn sanitize_body(body: &[u8]) -> String {
    if body.is_empty() {
        return "<empty>".to_string();
    }

    if let Ok(json) = serde_json::from_slice::<serde_json::Value>(body) {
        return sanitize_json_value(&json).to_string();
    }

    if let Ok(s) = std::str::from_utf8(body) {
        if s.contains('=') && !s.contains('{') && !s.contains('[') {
            let params: Vec<_> = s
                .split('&')
                .filter_map(|pair| {
                    let mut parts = pair.splitn(2, '=');
                    let key = parts.next()?;
                    let val = parts.next().unwrap_or("");
                    Some(format!("{}={}", key, sanitize_field_value(key, val)))
                })
                .take(20)
                .collect();
            if params.len() < 20 {
                return params.join("&");
            }
            return format!("{}... (truncated)", params.join("&"));
        }
    }

    if body.iter().take(100).any(|&b| b == 0 || b > 127) {
        return format!("<binary data: {} bytes>", body.len());
    }

    let s = std::str::from_utf8(body).unwrap_or("<invalid utf8>");
    if s.len() > 512 {
        format!("{}... ({} chars)", &s[..512], s.len())
    } else {
        s.to_string()
    }
}

/// 提取响应体日志并重建 Response（避免 body 被消费后无法返回）
///
/// 对 JSON 响应提取并打印预览，对非 JSON 响应显示大小。
/// 通过 `map_body` 重建 Response，将读取出的 body 重新封装回去。
async fn extract_and_log_response_body(response: &mut Response<Body>) -> String {
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_lowercase());

    let is_json = content_type
        .as_ref()
        .map(|ct| ct.contains("application/json"))
        .unwrap_or(false);

    if !is_json {
        let size = response.body().size_hint().upper().unwrap_or(u64::MAX);
        return format!("<non-json, {} bytes>", size);
    }

    let body = std::mem::replace(response.body_mut(), Body::empty());

    match axum::body::to_bytes(body, 5 * 1024 * 1024).await {
        Ok(bytes) => {
            let preview = if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                sanitize_json_value(&json).to_string()
            } else if let Ok(s) = std::str::from_utf8(&bytes) {
                if s.len() > 512 {
                    format!("{}... ({} chars)", &s[..512], s.len())
                } else {
                    s.to_string()
                }
            } else {
                format!("<binary: {} bytes>", bytes.len())
            };

            *response.body_mut() = Body::from(bytes);

            preview
        }
        Err(_) => {
            *response.body_mut() = Body::empty();
            "<failed to read body>".to_string()
        }
    }
}

/// 递归清理 JSON 中的敏感字段
fn sanitize_json_value(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let sanitized: serde_json::Map<String, serde_json::Value> = map
                .iter()
                .filter(|(key, _)| !is_sensitive_field(key))
                .map(|(key, val)| (key.clone(), sanitize_json_value(val)))
                .collect();
            serde_json::Value::Object(sanitized)
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(sanitize_json_value).collect())
        }
        serde_json::Value::String(s) => serde_json::Value::String(s.clone()),
        serde_json::Value::Number(n) => serde_json::Value::Number(n.clone()),
        serde_json::Value::Bool(b) => serde_json::Value::Bool(*b),
        serde_json::Value::Null => serde_json::Value::Null,
    }
}

/// 判断字段名是否为敏感字段
fn is_sensitive_field(name: &str) -> bool {
    const SENSITIVE_FIELDS: &[&str] = &[
        "password",
        "pwd",
        "secret",
        "token",
        "api_key",
        "apikey",
        "api-key",
        "private_key",
        "privatekey",
        "access_token",
        "access-token",
        "refresh_token",
        "refresh-token",
        "session_id",
        "sessionid",
        "csrf_token",
        "csrftoken",
        "x_csrf_token",
        "authorization",
        "credential",
        "credentials",
        "private",
        "encryption_key",
        "encryption-key",
        "secret_key",
        "secretkey",
        "auth",
        "signature",
        "cert",
        "certificate",
        "ssn",
        "credit_card",
        "creditcard",
        "cvv",
        "pin",
    ];

    let name_lower = name.to_lowercase();
    SENSITIVE_FIELDS.iter().any(|s| name_lower.contains(s))
}

/// 清理字段值，对敏感字段进行脱敏
fn sanitize_field_value(field_name: &str, value: &str) -> String {
    if is_sensitive_field(field_name) {
        let len = value.len();
        if len <= 4 {
            return "******".to_string();
        }
        return format!("{}...{}", &value[..2], &value[value.len() - 2..]);
    }
    if value.len() > 512 {
        format!("{}... (truncated)", &value[..512])
    } else {
        value.to_string()
    }
}
