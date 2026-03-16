//! 限流中间件

use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::RwLock;
use axum::{
    body::Body,
    extract::Request,
    middleware::Next,
    response::Response,
};
use crate::error::Error;

/// 限流配置
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    pub window_secs: u64,
    pub max_requests: u64,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            window_secs: 60,
            max_requests: 100,
        }
    }
}

impl RateLimitConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_window(mut self, secs: u64) -> Self {
        self.window_secs = secs;
        self
    }

    pub fn with_max_requests(mut self, n: u64) -> Self {
        self.max_requests = n;
        self
    }
}

/// 限流状态
#[derive(Clone)]
pub struct RateLimitState {
    inner: Arc<RwLock<HashMap<String, RateLimitBucket>>>,
}

struct RateLimitBucket {
    count: u64,
    window_start: Instant,
}

impl RateLimitState {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn check(&self, key: &str, config: &RateLimitConfig) -> Result<(), Error> {
        let mut state = self.inner.write().await;
        let now = Instant::now();
        let window = Duration::from_secs(config.window_secs);

        let bucket = state.entry(key.to_string()).or_insert_with(|| {
            RateLimitBucket {
                count: 0,
                window_start: now,
            }
        });

        if now.duration_since(bucket.window_start) >= window {
            bucket.count = 1;
            bucket.window_start = now;
            return Ok(());
        }

        if bucket.count >= config.max_requests {
            let elapsed = now.duration_since(bucket.window_start).as_secs();
            let retry_after = config.window_secs - elapsed;
            return Err(Error::rate_limit_exceeded(retry_after, config.max_requests, config.window_secs));
        }

        bucket.count += 1;
        Ok(())
    }
}

/// 限流中间件
pub async fn mw_rate_limit(
    mut req: Request<Body>,
    next: Next,
) -> Response {
    let config = RateLimitConfig::default();
    let state = RateLimitState::new();

    let key = req.headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(',').next().unwrap_or(s).to_string())
        .unwrap_or_else(|| "default".to_string());

    if let Err(e) = state.check(&key, &config).await {
        return Error::into_rate_limit_response(e);
    }

    next.run(req).await
}

/// 创建限流配置
pub fn rate_limit_layer(window_secs: u64, max_requests: u64) -> RateLimitConfig {
    RateLimitConfig::new()
        .with_window(window_secs)
        .with_max_requests(max_requests)
}

/// 宽松限流
pub fn rate_limit_layer_permissive() -> RateLimitConfig {
    rate_limit_layer(60, 1000)
}

/// 严格限流
pub fn rate_limit_layer_strict() -> RateLimitConfig {
    rate_limit_layer(60, 60)
}
