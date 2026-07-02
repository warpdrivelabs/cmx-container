//! 请求体/响应体脱敏工具模块。
//!
//! 提供敏感数据过滤和脱敏处理，包括请求头过滤、请求体清理、JSON 递归脱敏。

use axum::http::HeaderMap;

/// 收集请求头，排除敏感字段。
///
/// 过滤的请求头包括：Authorization、Cookie、Set-Cookie、User-Agent 等。
///
/// # Arguments
///
/// * `headers` - HTTP 请求头
///
/// # Returns
///
/// 非敏感请求头的键值对列表。
pub fn collect_headers(headers: &HeaderMap) -> Vec<(String, String)> {
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

/// 清理请求体，排除敏感字段和二进制数据。
///
/// 处理策略：
/// - 空 body 返回 `<empty>`
/// - JSON body 递归脱敏后输出
/// - URL-encoded body 逐字段脱敏，最多 20 个参数
/// - 二进制 body 显示字节数
/// - 纯文本 body 超过 512 字符时截断
///
/// # Arguments
///
/// * `body` - 原始请求体字节
///
/// # Returns
///
/// 脱敏后的请求体预览字符串。
pub fn sanitize_body(body: &[u8]) -> String {
    if body.is_empty() {
        return "<empty>".to_string();
    }

    if let Ok(json) = serde_json::from_slice::<serde_json::Value>(body) {
        return sanitize_json_value(&json).to_string();
    }

    if let Ok(s) = std::str::from_utf8(body)
        && s.contains('=')
        && !s.contains('{')
        && !s.contains('[')
    {
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

/// 递归清理 JSON 中的敏感字段。
///
/// 移除键名匹配敏感词列表的字段，对数组和嵌套对象递归处理。
///
/// # Arguments
///
/// * `value` - JSON 值
///
/// # Returns
///
/// 脱敏后的 JSON 值，敏感字段已被移除。
pub fn sanitize_json_value(value: &serde_json::Value) -> serde_json::Value {
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
        other => other.clone(),
    }
}

/// 判断字段名是否为敏感字段。
///
/// 敏感字段包括：password、token、secret、api_key、credential 等，
/// 匹配方式为字段名（小写）包含敏感词。
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

/// 清理字段值，对敏感字段进行脱敏。
///
/// 敏感字段值长度大于 4 时保留首尾各 2 字符，中间用 `...` 替代；
/// 长度不超过 4 时全部替换为 `******`。非敏感字段超过 512 字符时截断。
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
