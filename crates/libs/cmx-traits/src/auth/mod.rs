//! 认证领域 trait 抽象。
//!
//! 包含认证服务、认证策略、用户认证数据查询、Auth 表存储查询等接口。
//!
//! # 模块组织
//!
//! - [`context_scope`] — 认证上下文请求级传播（task_local）。
//! - [`error`] — 认证领域错误类型（AuthError）。
//! - [`policy`] — 认证策略 trait（AuthPolicy）。
//! - [`service`] — 认证服务统一接口（AuthService）。
//! - [`storage_query`] — Auth 表存储查询 trait（AuthStorageQuery）。
//! - [`user_query`] — 用户认证数据查询 trait（UserAuthQuery）。

pub mod context_scope;
pub mod error;
pub mod policy;
pub mod service;
pub mod storage_query;
pub mod user_query;

pub use context_scope::{CallerIdentity, RequestAuth};
pub use error::AuthError;
pub use policy::AuthPolicy;
pub use service::{
    AuthService, Credentials, DeviceInfo, OAuth2CallbackExchangeResult, OAuth2CallbackResult,
    TokenPair, UserInfo,
};
pub use storage_query::AuthStorageQuery;
pub use user_query::{
    ApiKeyData, OAuth2ClientData, OAuth2UserInfo, ProviderInfo, UserAuthData, UserAuthQuery,
};
