//! 安全响应头中间件

use axum::{
    body::Body,
    extract::Request,
    middleware::Next,
    response::Response,
};

/// 安全头配置
#[derive(Debug, Clone)]
pub struct SecurityHeadersConfig {
    pub content_type_options: &'static str,
    pub frame_options: &'static str,
    pub xss_protection: &'static str,
    pub hsts_max_age: Option<u64>,
    pub referrer_policy: &'static str,
}

impl Default for SecurityHeadersConfig {
    fn default() -> Self {
        Self {
            content_type_options: "nosniff",
            frame_options: "DENY",
            xss_protection: "1; mode=block",
            hsts_max_age: Some(31536000),
            referrer_policy: "strict-origin-when-cross-origin",
        }
    }
}

impl SecurityHeadersConfig {
    pub fn new() -> Self {
        Self::default()
    }
}

/// 添加安全头到响应
pub async fn mw_security_headers(
    mut req: Request<Body>,
    next: Next,
) -> Response {
    let mut response = next.run(req).await;

    response.headers_mut().insert(
        "x-content-type-options",
        "nosniff".parse().unwrap(),
    );
    response.headers_mut().insert(
        "x-frame-options",
        "DENY".parse().unwrap(),
    );
    response.headers_mut().insert(
        "x-xss-protection",
        "1; mode=block".parse().unwrap(),
    );
    response.headers_mut().insert(
        "referrer-policy",
        "strict-origin-when-cross-origin".parse().unwrap(),
    );
    response.headers_mut().insert(
        "strict-transport-security",
        "max-age=31536000".parse().unwrap(),
    );

    response
}
