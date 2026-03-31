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
use tracing::{ info, warn};

/// 请求追踪中间件
///
/// 记录以下信息：
/// - 请求方法、路径、查询参数
/// - 请求头（排除敏感字段）
/// - 请求体（排除文件和二进制字段）
/// - 响应状态码和处理耗时
///
/// 排除的请求头：Authorization、Cookie、Set-Cookie、Sec-WebSocket-Key 等
/// 排除的请求字段：password、secret、token、file、binary 等（不区分大小写）
pub async fn mw_trace(req: Request<Body>, next: Next) -> Response {
    let start = Instant::now();
    let method = req.method().clone();
    let uri = req.uri().clone();
    let path = uri.path().to_string();
    let query = uri.query().map(|s| s.to_string());

    let headers = collect_headers(req.headers());

    // 【关键步骤 1】拆解 Request，分离出 Parts 和 Body
    // - into_parts() 会消耗原始 Request，但能让我们访问 Body 流
    // - Body 是单向异步流，只能消费一次，需要读取并缓存
    let (parts, body) = req.into_parts();

    // 【关键步骤 2】消费 Body 流，读取为字节向量（最多 10MB）
    // - extract_body 会异步读取整个 Body 到内存
    // - 排除 multipart/form-data（文件上传场景）
    let body_bytes = extract_body(&parts, body).await;

    // 【关键步骤 3】清理和预览请求体（脱敏、截断等）
    let req_body_preview = sanitize_body(&body_bytes);

    // 【关键步骤 4】重建 Request，供后续中间件使用
    // - from_parts() 用 Parts + 新的 Body 重新组装完整的 Request
    // - body_bytes.clone() 确保传递给 next 的是完整副本
    // ⚠️ 如果不重建，后续中间件将无法访问请求体
    let new_req = Request::from_parts(parts, Body::from(body_bytes.clone()));

    // 【关键步骤 5】执行后续中间件链，传递重建后的 Request
    let response = next.run(new_req).await;
    let duration = start.elapsed();

    let status = response.status();
    let response_size = response
        .body()
        .size_hint()
        .upper()
        .unwrap_or(u64::MAX);

    info!(
        target: "req_trace",
        "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n\
         📥 REQUEST\n\
         ┣ path: {} {}\n\
         ┣ query: {:?}\n\
         ┣ headers: {:?}\n\
         ┣ body_preview: {}\n\
         ┗ duration: {:?}\n\
         📤 RESPONSE\n\
         ┣ status: {}\n\
         ┣ response_size: {} bytes\n\
         ┗ duration: {:?}",
        method,
        path,
        query,
        headers,
        req_body_preview,
        duration,
        status.as_u16(),
        response_size,
        duration
    );

    if status.is_server_error() || status.as_u16() >= 500 {
        warn!(
            target: "req_trace",
            "⚠️  SERVER ERROR - {} {} - {}",
            method,
            path,
            status.as_u16()
        );
    }

    info!(
        target: "req_trace",
        "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    );

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

/// 提取请求体，返回字节向量
async fn extract_body(parts: &axum::http::request::Parts, body: Body) -> Vec<u8> {
    let content_type = parts
        .headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_lowercase());

    let is_multipart = content_type
        .as_ref()
        .map(|ct| ct.contains("multipart/form-data"))
        .unwrap_or(false);

    if is_multipart {
        return Vec::new();
    }

    match axum::body::to_bytes(body, 10 * 1024 * 1024).await {
        Ok(bytes) => bytes.to_vec(),
        Err(_) => Vec::new(),
    }
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
