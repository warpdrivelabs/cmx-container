//! 限流中间件（**已停用 · 冻结待办**）
//!
//! 本文件整体注释停用。当前架构决策：**应用层不做限流，统一由网关/接入层承担**
//! （反向代理 / API 网关 / Nacos 前置的流量治理），理由：
//!   - 应用为多实例无状态部署，进程内计数器无法跨实例统一限流（会各算各的）；
//!   - 网关侧限流可与鉴权、熔断、灰度等流量策略集中配置，避免逻辑散落到每个服务。
//!
//! 若未来确需应用内兜底限流，应改为基于共享存储（如 Redis 令牌桶）的分布式实现，
//! 而非下方这套进程内 `HashMap<..., Instant>` 方案（仅单实例有效，故停用）。
//! 保留代码供参考，启用前请先落实分布式方案与网关分工边界。

// //! 限流中间件
//
// use std::{
//     collections::HashMap,
//     sync::Arc,
//     time::{Duration, Instant},
// };
// use tokio::sync::RwLock;
// use axum::{
//     body::Body,
//     extract::Request,
//     middleware::Next,
//     response::Response,
// };
// use crate::Error;
// 
// /// 限流配置
// #[derive(Debug, Clone)]
// pub struct RateLimitConfig {
//     pub window_secs: u64,
//     pub max_requests: u64,
// }
// 
// impl Default for RateLimitConfig {
//     fn default() -> Self {
//         Self {
//             window_secs: 60,
//             max_requests: 100,
//         }
//     }
// }
// 
// impl RateLimitConfig {
//     pub fn new() -> Self {
//         Self::default()
//     }
// 
//     pub fn with_window(mut self, secs: u64) -> Self {
//         self.window_secs = secs;
//         self
//     }
// 
//     pub fn with_max_requests(mut self, n: u64) -> Self {
//         self.max_requests = n;
//         self
//     }
// }
// 
// /// 限流状态
// #[derive(Clone)]
// pub struct RateLimitState {
//     inner: Arc<RwLock<HashMap<String, RateLimitBucket>>>,
// }
// 
// struct RateLimitBucket {
//     count: u64,
//     window_start: Instant,
// }
// 
// impl Default for RateLimitState {
//     fn default() -> Self {
//         Self::new()
//     }
// }
// 
// impl RateLimitState {
//     pub fn new() -> Self {
//         Self {
//             inner: Arc::new(RwLock::new(HashMap::new())),
//         }
//     }
// 
//     pub async fn check(&self, key: &str, config: &RateLimitConfig) -> Result<(), Error> {
//         let mut state = self.inner.write().await;
//         let now = Instant::now();
//         let window = Duration::from_secs(config.window_secs);
// 
//         let bucket = state.entry(key.to_string()).or_insert_with(|| {
//             RateLimitBucket {
//                 count: 0,
//                 window_start: now,
//             }
//         });
// 
//         if now.duration_since(bucket.window_start) >= window {
//             bucket.count = 1;
//             bucket.window_start = now;
//             return Ok(());
//         }
// 
//         if bucket.count >= config.max_requests {
//             let elapsed = now.duration_since(bucket.window_start).as_secs();
//             let retry_after = config.window_secs - elapsed;
//             return Err(Error::rate_limit_exceeded(retry_after, config.max_requests, config.window_secs));
//         }
// 
//         bucket.count += 1;
//         Ok(())
//     }
// }
// 
// /// 限流中间件
// pub async fn mw_rate_limit(
//     req: Request<Body>,
//     next: Next,
// ) -> Response {
//     let config = RateLimitConfig::default();
//     let state = RateLimitState::new();
// 
//     let key = req.headers()
//         .get("x-forwarded-for")
//         .and_then(|v| v.to_str().ok())
//         .map(|s| s.split(',').next().unwrap_or(s).to_string())
//         .unwrap_or_else(|| "default".to_string());
// 
//     if let Err(e) = state.check(&key, &config).await {
//         return Error::into_rate_limit_response(e);
//     }
// 
//     next.run(req).await
// }
// 
// /// 创建限流配置
// pub fn rate_limit_layer(window_secs: u64, max_requests: u64) -> RateLimitConfig {
//     RateLimitConfig::new()
//         .with_window(window_secs)
//         .with_max_requests(max_requests)
// }
// 
// /// 宽松限流
// pub fn rate_limit_layer_permissive() -> RateLimitConfig {
//     rate_limit_layer(60, 1000)
// }
// 
// /// 严格限流
// pub fn rate_limit_layer_strict() -> RateLimitConfig {
//     rate_limit_layer(60, 60)
// }
