//! JWT Bearer 认证策略

use async_trait::async_trait;
use cmx_core::AuthContext;
use cmx_traits::auth::{AuthError, AuthPolicy};

use crate::jwt::JwtManager;
use crate::token::TokenManager;

/// JWT Bearer 认证策略。
///
/// 解码 Access Token、检查黑名单并构建 `AuthContext`，
/// 不做会话活跃度检查（由调用方决定）。
pub struct JwtBearerPolicy {
    /// JWT 编解码器。
    jwt_manager: JwtManager,

    /// Token 管理器（提供黑名单查询）。
    token_manager: TokenManager,
}

impl JwtBearerPolicy {
    /// 创建新的 JWT Bearer 策略
    pub fn new(jwt_manager: JwtManager, token_manager: TokenManager) -> Self {
        Self {
            jwt_manager,
            token_manager,
        }
    }
}

#[async_trait]
impl AuthPolicy for JwtBearerPolicy {
    fn name(&self) -> &str {
        "jwt_bearer"
    }

    async fn authenticate(&self, token: &str) -> Result<AuthContext, AuthError> {
        // 解码 Token
        let claims = self.jwt_manager.decode_access_token(token)?;

        // 检查黑名单
        if self.token_manager.is_blacklisted(&claims.jti).await? {
            return Err(AuthError::TokenRevoked);
        }

        // 构建 AuthContext
        Ok(AuthContext {
            user_id: claims.sub,
            username: claims.username,
            roles: claims.roles,
            permissions: claims.permissions,
            org_id: claims.org_id,
            session_id: Some(claims.sid),
            device_type: Some(claims.device),
            auth_method: Some("jwt_bearer".to_string()),
        })
    }
}
