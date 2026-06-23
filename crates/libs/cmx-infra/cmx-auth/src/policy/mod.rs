//! 认证策略模块。
//!
//! 实现 `AuthPolicy` trait 的两种策略：JWT Bearer 和 OAuth2。

pub mod jwt_policy;
pub mod oauth2_policy;

pub use jwt_policy::JwtBearerPolicy;
pub use oauth2_policy::OAuth2Policy;
