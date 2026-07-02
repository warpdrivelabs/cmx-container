//! 认证基础设施错误类型。
//!
//! 保留完整错误链（Redis/JWT/Database），在返回 `AuthService` trait 接口时
//! 通过 `.map_err()` 转换为 `cmx-traits::AuthError`。

use thiserror::Error;

/// 认证基础设施错误（保留完整错误链）。
///
/// `cmx-auth` 内部实现使用此类型，
/// 在返回 `AuthService` trait 接口时通过 `.map_err()` 转换为 `AuthError`。
#[derive(Debug, Error)]
pub enum AuthInfraError {
    /// Redis 操作错误。
    #[error("Redis 操作错误")]
    Redis(#[from] cmx_buffer::error::Error),

    /// JWT 错误。
    #[error("JWT 错误")]
    Jwt(#[from] jsonwebtoken::errors::Error),

    /// 数据库操作错误。
    #[error("数据库操作错误")]
    Database(#[from] cmx_database::error::Error),

    /// 序列化错误。
    #[error("序列化错误: {0}")]
    SerdeJson(#[from] serde_json::Error),

    /// Prometheus 指标错误。
    #[error("Prometheus 指标错误")]
    Prometheus(#[from] prometheus::Error),

    /// 认证领域错误。
    #[error(transparent)]
    Auth(#[from] cmx_traits::auth::AuthError),
}

impl From<AuthInfraError> for cmx_traits::auth::AuthError {
    fn from(e: AuthInfraError) -> Self {
        match e {
            AuthInfraError::Auth(auth_err) => auth_err,
            AuthInfraError::Redis(err) => cmx_traits::auth::AuthError::Internal(err.to_string()),
            AuthInfraError::Jwt(err) => cmx_traits::auth::AuthError::InvalidToken(err.to_string()),
            AuthInfraError::Database(err) => cmx_traits::auth::AuthError::Internal(err.to_string()),
            AuthInfraError::SerdeJson(err) => {
                cmx_traits::auth::AuthError::Internal(err.to_string())
            }
            AuthInfraError::Prometheus(err) => {
                cmx_traits::auth::AuthError::Internal(err.to_string())
            }
        }
    }
}

/// `cmx-auth` 内部 `Result` 类型别名。
pub type Result<T> = core::result::Result<T, AuthInfraError>;
