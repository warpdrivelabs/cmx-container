//! 认证领域错误类型。
//!
//! 定义在 cmx-traits 中，使 AuthService trait 可返回强类型错误，
//! 避免 `Box<dyn Error>` 导致类型擦除和错误映射失效。
//! cmx-traits 不依赖 cmx-buffer/jsonwebtoken 等具体实现库，
//! 底层错误链由 cmx-auth 的 AuthInfraError 保留。

use thiserror::Error;

/// 认证领域错误（仅保留领域语义，不含基础设施错误链）。
#[derive(Debug, Error)]
pub enum AuthError {
    /// 用户名或密码错误。
    #[error("用户名或密码错误")]
    InvalidCredentials,

    /// Token 无效。
    #[error("Token 无效: {0}")]
    InvalidToken(String),

    /// Token 已过期。
    #[error("Token 已过期")]
    TokenExpired,

    /// Token 已被撤销。
    #[error("Token 已被撤销")]
    TokenRevoked,

    /// Refresh Token 已失效，检测到重放攻击。
    #[error("Refresh Token 已失效，检测到重放攻击")]
    ReplayDetected,

    /// 会话不存在或已过期。
    #[error("会话不存在或已过期")]
    SessionNotFound,

    /// 密码哈希失败。
    #[error("密码哈希失败: {0}")]
    PasswordHashError(String),

    /// 密码校验失败。
    #[error("密码校验失败")]
    PasswordVerifyFailed,

    /// 密码不符合策略要求。
    #[error("密码不符合策略要求: {0}")]
    PasswordPolicyViolated(String),

    /// 密码与历史密码重复。
    #[error("密码与历史密码重复")]
    PasswordReused,

    /// OAuth2 错误。
    #[error("OAuth2 错误: {0}")]
    OAuth2(String),

    /// OAuth2 客户端不存在。
    #[error("OAuth2 客户端不存在: {0}")]
    ClientNotFound(String),

    /// 授权码无效或已过期。
    #[error("授权码无效或已过期")]
    InvalidAuthCode,

    /// PKCE 校验失败。
    #[error("PKCE 校验失败")]
    PkceVerificationFailed,

    /// 权限不足。
    #[error("权限不足")]
    Forbidden,

    /// 用户已被禁用。
    #[error("用户已被禁用")]
    UserDisabled,

    /// API Key 无效。
    #[error("API Key 无效")]
    InvalidApiKey,

    /// API Key 已过期。
    #[error("API Key 已过期")]
    ApiKeyExpired,

    /// 登录失败次数过多，账号已锁定。
    #[error("登录失败次数过多，账号已锁定 {secs} 秒")]
    TooManyAttempts {
        /// 锁定剩余秒数。
        secs: u64,
        /// 锁定阈值。
        limit: u32,
        /// 锁定窗口（秒）。
        window: u64,
    },

    /// OAuth2 Provider 不存在。
    #[error("OAuth2 Provider 不存在: {0}")]
    OAuth2ProviderNotFound(String),

    /// OAuth2 Provider 服务不可达。
    #[error("OAuth2 Provider 服务不可达: {0}")]
    OAuth2ProviderUnavailable(String),

    /// OAuth2 Provider Token 交换失败。
    #[error("OAuth2 Provider Token 交换失败: {0}")]
    OAuth2ProviderTokenError(String),

    /// OAuth2 Provider 用户信息获取失败。
    #[error("OAuth2 Provider 用户信息获取失败: {0}")]
    OAuth2ProviderUserInfoError(String),

    /// 第三方账号未绑定本地用户。
    #[error("第三方账号未绑定本地用户: {provider}:{provider_user_id}")]
    OAuth2AccountNotLinked {
        /// Provider 名称。
        provider: String,
        /// Provider 侧用户唯一标识。
        provider_user_id: String,
    },

    /// Provider 邮箱未验证，无法自动关联。
    #[error("Provider 邮箱未验证，无法自动关联")]
    OAuth2EmailNotVerified,

    /// 无法解除最后一个登录绑定。
    #[error("无法解除最后一个登录绑定")]
    OAuth2LastBindingCannotRemove,

    /// 用户名冲突，自动注册失败。
    #[error("用户名冲突，自动注册失败: {0}")]
    OAuth2UsernameConflict(String),

    /// 回调授权码无效或已过期。
    #[error("第三方 OAuth2 回调授权码无效或已过期")]
    OAuth2CallbackCodeInvalid,

    /// 序列化/反序列化错误。
    #[error("序列化错误: {0}")]
    SerdeJson(#[from] serde_json::Error),

    /// 内部错误。
    #[error("内部错误: {0}")]
    Internal(String),
}
