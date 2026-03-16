//! CORS 中间件配置

use tower_http::cors::{CorsLayer, ExposeHeaders, AllowOrigin};

/// CORS 配置
#[derive(Debug, Clone)]
pub struct CorsConfig {
    allow_origins: AllowOrigin,
    allow_methods: Vec<axum::http::Method>,
    allow_headers: Vec<axum::http::HeaderName>,
    expose_headers: Vec<axum::http::HeaderName>,
    allow_credentials: bool,
    max_age: u64,
}

impl Default for CorsConfig {
    fn default() -> Self {
        Self {
            allow_origins: AllowOrigin::any(),
            allow_methods: vec![
                axum::http::Method::GET,
                axum::http::Method::POST,
                axum::http::Method::PUT,
                axum::http::Method::DELETE,
                axum::http::Method::PATCH,
                axum::http::Method::OPTIONS,
            ],
            allow_headers: vec![
                axum::http::header::CONTENT_TYPE,
                axum::http::header::AUTHORIZATION,
                axum::http::header::ACCEPT,
            ],
            expose_headers: vec![
                axum::http::header::CONTENT_LENGTH,
            ],
            allow_credentials: true,
            max_age: 3600,
        }
    }
}

impl CorsConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn build(self) -> CorsLayer {
        CorsLayer::new()
            .allow_origin(self.allow_origins)
            .allow_methods(self.allow_methods)
            .allow_headers(self.allow_headers.clone())
            .expose_headers(ExposeHeaders::list(self.expose_headers))
            .allow_credentials(self.allow_credentials)
            .max_age(std::time::Duration::from_secs(self.max_age))
    }
}

/// 创建默认 CORS 层
pub fn cors_layer() -> CorsLayer {
    CorsConfig::new().build()
}

/// 创建宽松的 CORS 层
pub fn cors_layer_permissive() -> CorsLayer {
    CorsLayer::permissive()
}
