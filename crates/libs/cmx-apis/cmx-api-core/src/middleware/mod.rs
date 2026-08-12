//! 中间件模块

pub mod mw_auth;
pub mod mw_context;
pub mod mw_cors;
pub mod mw_permission;
pub mod mw_security_headers;
// pub mod mw_rate_limit;
pub mod mw_trace;
pub mod trace;

pub use mw_auth::{GlobalAuthService, mw_auth};
pub use mw_context::{CmxSvrContext, mw_context_resolver};
pub use mw_cors::{CorsConfig, cors_layer, cors_layer_permissive};
pub use mw_permission::{GlobalPermissionConfig, mw_permission};
// pub use mw_rate_limit::{mw_rate_limit, rate_limit_layer, rate_limit_layer_permissive, rate_limit_layer_strict, RateLimitConfig};
pub use mw_security_headers::{SecurityHeadersConfig, mw_security_headers};
pub use mw_trace::mw_trace;
pub use trace::trace_layer;
