//! 认证领域 trait 抽象
//!
//! 包含认证服务、认证策略、用户认证数据查询、Auth 表存储查询等接口。
//!
//! # 模块组织
//!
//! - [`error`] — 认证领域错误类型（AuthError）
//! - [`policy`] — 认证策略 trait（AuthPolicy）
//! - [`service`] — 认证服务统一接口（AuthService）
//! - [`storage_query`] — Auth 表存储查询 trait（AuthStorageQuery）
//! - [`user_query`] — 用户认证数据查询 trait（UserAuthQuery）

pub mod error;
pub mod policy;
pub mod service;
pub mod storage_query;
pub mod user_query;

pub use error::AuthError;
pub use policy::AuthPolicy;
pub use service::{
    AuthService, Credentials, TokenPair, DeviceInfo, OAuth2CallbackResult, OAuth2CallbackExchangeResult,
};
pub use storage_query::AuthStorageQuery;
pub use user_query::{
    UserAuthQuery, UserAuthData, ApiKeyData, OAuth2ClientData, OAuth2UserInfo, ProviderInfo,
};
