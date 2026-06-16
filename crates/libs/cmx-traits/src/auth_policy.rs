//! 认证策略抽象
//!
//! 定义 AuthPolicy trait（Strategy Pattern），支持多种认证方式：
//! JWT Bearer / API Key / OAuth2。

use async_trait::async_trait;
use cmx_core::AuthContext;

use crate::auth_error::AuthError;

/// 认证策略 trait（Strategy Pattern）
///
/// 每种认证方式实现此 trait，AuthService 根据凭证类型分发。
#[async_trait]
pub trait AuthPolicy: Send + Sync {
    /// 策略名称（用于日志和指标）
    fn name(&self) -> &str;

    /// 校验凭证，返回 AuthContext
    async fn authenticate(&self, credential: &str) -> Result<AuthContext, AuthError>;
}
