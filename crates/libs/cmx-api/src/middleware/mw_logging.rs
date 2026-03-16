//! 日志中间件

use axum::{
    body::Body,
    extract::Request,
    middleware::Next,
    response::Response,
};
use std::time::{Duration, Instant};
use tracing::{info, warn};

/// 日志配置
#[derive(Debug, Clone, Default)]
pub struct LogConfig {
    pub slow_threshold_ms: u64,
}

impl LogConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_slow_threshold(mut self, ms: u64) -> Self {
        self.slow_threshold_ms = ms;
        self
    }
}

/// 请求日志中间件
pub async fn mw_logging(
    req: Request<Body>,
    next: Next,
) -> Response {
    let start = Instant::now();
    let method = req.method().clone();
    let uri = req.uri().clone();

    info!(">>> {} {}", method, uri);

    let response = next.run(req).await;
    let duration = start.elapsed();

    let status = response.status();

    if duration > Duration::from_millis(3000) {
        warn!("<<< {} {} - {} ({:?}) [SLOW]", method, uri, status.as_u16(), duration);
    } else {
        info!("<<< {} {} - {} ({:?})", method, uri, status.as_u16(), duration);
    }

    response
}

/// 带配置的日志中间件
pub async fn mw_logging_with_config(
    req: Request<Body>,
    next: Next,
    _config: LogConfig,
) -> Response {
    mw_logging(req, next).await
}
