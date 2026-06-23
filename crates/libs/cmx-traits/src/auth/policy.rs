//! 认证策略抽象。
//!
//! 定义 AuthPolicy trait（Strategy Pattern），支持多种认证方式：
//! JWT Bearer / API Key / OAuth2。

use async_trait::async_trait;
use cmx_core::AuthContext;

use super::error::AuthError;

/// 认证策略 trait（Strategy Pattern）。
///
/// 每种认证方式实现此 trait，AuthService 根据凭证类型分发。
#[async_trait]
pub trait AuthPolicy: Send + Sync {
    /// 返回策略名称（用于日志和指标）。
    fn name(&self) -> &str;

    /// 校验凭证并返回认证上下文。
    ///
    /// # Arguments
    ///
    /// * `credential` - 待校验的凭证字符串，格式由具体策略定义。
    ///
    /// # Returns
    ///
    /// 成功时返回 [`AuthContext`]，凭证无效时返回对应的 [`AuthError`]。
    ///
    /// # Errors
    ///
    /// * [`AuthError::InvalidCredentials`] - 凭证格式或内容无效。
    /// * [`AuthError::InvalidToken`] - Token 无效或已撤销。
    /// * [`AuthError::TokenExpired`] - Token 已过期。
    async fn authenticate(&self, credential: &str) -> Result<AuthContext, AuthError>;
}
