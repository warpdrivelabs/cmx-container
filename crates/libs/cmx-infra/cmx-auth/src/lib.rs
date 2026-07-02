//! cmx-auth — 企业级统一认证模块
//!
//! 提供完整的认证基础设施，包括：
//! - JWT 双令牌（Access Token + Refresh Token）
//! - Refresh Token Rotation（轮换防重放，Lua 原子操作）
//! - Argon2id 密码哈希 + 密码策略 + 历史校验
//! - OAuth2 Authorization Code Flow + PKCE
//! - 会话管理（SSO/互踢/心跳/在线统计）
//! - API Key 管理（配置文件 + API 双通道）
//! - 认证策略模式（JWT Bearer / API Key / OAuth2）
//! - Prometheus 指标 + Tracing span 可观测性

pub mod auth_service_impl;
pub mod config;
pub mod error;
pub mod jwt;
pub mod oauth2;
pub mod password;
pub mod policy;
pub mod session;
pub mod token;

pub mod api_key;
pub mod metrics;

pub use auth_service_impl::AuthServiceImpl;
pub use config::AccountLinkConfig;
pub use config::AuthConfig;
pub use config::OAuth2ProviderConfig;
pub use config::StaticApiKeyConfig;
pub use config::SuperAdminConfig;
pub use error::AuthInfraError;
