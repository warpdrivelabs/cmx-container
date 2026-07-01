//! 用户信息查询与审计日志辅助
//!
//! 实现 [`cmx_traits::auth::AuthService::get_user_info`]（聚合用户/角色/权限），
//! 以及内部审计日志记录辅助方法（供登录、密码修改、OAuth2 等子模块复用）。

use cmx_traits::auth::{AuthError, UserInfo};
use tracing::warn;

use crate::auth_service_impl::AuthServiceImpl;

impl AuthServiceImpl {
    /// 4.5: 记录审计日志
    pub(super) async fn audit_log(
        &self,
        operation: &str,
        result: cmx_audit::OperationResult,
        actor_id: &str,
        target_type: Option<&str>,
        target_id: Option<&str>,
        details: Option<serde_json::Value>,
    ) {
        if let Some(ref logger) = self.audit_logger {
            let mut record = cmx_audit::AuditRecord::new(
                cmx_audit::AuditDomain::Auth,
                operation,
                result,
            )
            .with_actor(actor_id, "");

            if let Some(tt) = target_type {
                record = record.with_target(tt, target_id.unwrap_or(""));
            }
            if let Some(d) = details {
                record = record.with_details(d);
            }

            if let Err(e) = logger.log(record).await {
                warn!(operation = operation, error = %e, "审计日志记录失败");
            }
        }
    }

    /// 获取当前登录用户的完整信息（含 nickname/email/roles/permissions）。
    ///
    /// 从 `cmx_user` 表查询用户基本信息，并附加角色、权限列表。
    /// 用于 `/api/auth/me` 接口。
    ///
    /// # Arguments
    ///
    /// * `user_id` - 目标用户 ID。
    ///
    /// # Returns
    ///
    /// 成功时返回 `UserInfo`，包含用户基本信息与角色权限。
    ///
    /// # Errors
    ///
    /// * `AuthError::InvalidToken` - 用户不存在。
    /// * `AuthError::UserDisabled` - 用户已禁用。
    /// * `AuthError::Internal` - 数据库查询失败。
    pub(super) async fn get_user_info(&self, user_id: &str) -> std::result::Result<UserInfo, AuthError> {
        let user = self
            .user_query
            .get_user_by_id(user_id)
            .await
            .map_err(|e| AuthError::Internal(e.to_string()))?
            .ok_or(AuthError::InvalidToken("用户不存在".to_string()))?;

        if user.status == 0 {
            return Err(AuthError::UserDisabled);
        }

        let roles = self
            .user_query
            .get_user_role_codes(user_id)
            .await
            .map_err(|e| AuthError::Internal(e.to_string()))?;
        let permissions = self
            .user_query
            .get_user_permissions(user_id)
            .await
            .map_err(|e| AuthError::Internal(e.to_string()))?;

        Ok(UserInfo {
            user_id: user.user_id,
            username: user.username,
            nickname: user.nickname,
            email: user.email,
            phone: user.phone,
            avatar: user.avatar,
            org_id: user.org_id,
            gender: user.gender,
            last_login_at: user.last_login_at,
            last_login_ip: user.last_login_ip,
            description: user.description,
            roles,
            permissions,
            session_id: None,
            device_type: None,
            auth_method: None,
        })
    }
}
