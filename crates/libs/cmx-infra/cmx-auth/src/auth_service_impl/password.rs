//! 密码哈希与修改
//!
//! 实现 [`cmx_traits::auth::AuthService`] 的密码哈希、校验与修改（含策略+历史校验链）。

use cmx_traits::auth::AuthError;
use tracing::info;

use crate::auth_service_impl::AuthServiceImpl;

impl AuthServiceImpl {
    /// 对明文密码进行 Argon2id 哈希。
    ///
    /// # Arguments
    ///
    /// * `plain` - 待哈希的明文密码。
    ///
    /// # Returns
    ///
    /// 成功时返回 Argon2id 哈希字符串（含盐值与参数）。
    ///
    /// # Errors
    ///
    /// 当 Argon2 哈希失败时返回 `AuthError::PasswordHashError`。
    pub(super) async fn hash_password(
        &self,
        plain: &str,
    ) -> std::result::Result<String, AuthError> {
        self.password_hasher
            .hash(plain)
            .map_err(|e| AuthError::PasswordHashError(e.to_string()))
    }

    /// 校验明文密码与哈希是否匹配。
    ///
    /// # Arguments
    ///
    /// * `plain` - 待校验的明文密码。
    /// * `hash` - 已存储的 Argon2id 哈希字符串。
    ///
    /// # Returns
    ///
    /// 匹配时返回 `true`，不匹配返回 `false`。
    ///
    /// # Errors
    ///
    /// 当哈希字符串解析失败时返回 `AuthError::PasswordVerifyFailed`。
    pub(super) async fn verify_password(
        &self,
        plain: &str,
        hash: &str,
    ) -> std::result::Result<bool, AuthError> {
        self.password_hasher
            .verify(plain, hash)
            .map_err(|_| AuthError::PasswordVerifyFailed)
    }

    /// 修改密码（含完整校验链）。
    ///
    /// 校验旧密码 → 校验新密码策略 → 校验密码历史 → 哈希新密码 →
    /// 记录历史 → 持久化 → 强制下线所有旧会话。
    ///
    /// # Arguments
    ///
    /// * `user_id` - 目标用户 ID。
    /// * `old_password` - 当前明文密码。
    /// * `new_password` - 新明文密码。
    ///
    /// # Errors
    ///
    /// * `AuthError::PasswordPolicyViolated` - 新密码与旧密码相同或不符合策略。
    /// * `AuthError::InvalidCredentials` - 旧密码错误或用户无密码。
    /// * `AuthError::PasswordReused` - 新密码在历史中已使用。
    pub(super) async fn change_password(
        &self,
        user_id: &str,
        old_password: &str,
        new_password: &str,
    ) -> std::result::Result<(), AuthError> {
        // 5.4: 显式校验新旧密码不能相同
        if old_password == new_password {
            return Err(AuthError::PasswordPolicyViolated(
                "新密码不能与当前密码相同".to_string(),
            ));
        }

        // 1. 查询用户
        let user = self
            .user_query
            .get_user_by_id(user_id)
            .await
            .map_err(|e| AuthError::Internal(e.to_string()))?
            .ok_or(AuthError::InvalidCredentials)?;

        // 2. 校验旧密码
        let password_hash = user
            .password_hash
            .as_ref()
            .ok_or(AuthError::InvalidCredentials)?;
        let valid = self
            .password_hasher
            .verify(old_password, password_hash)
            .map_err(|_| AuthError::PasswordVerifyFailed)?;
        if !valid {
            return Err(AuthError::InvalidCredentials);
        }

        // 3. 密码策略校验
        self.password_policy.validate(new_password)?;

        // 4. 密码历史校验
        let reused = self
            .password_history
            .is_reused(user_id, new_password)
            .await
            .map_err(|e| AuthError::Internal(e.to_string()))?;
        if reused {
            return Err(AuthError::PasswordReused);
        }

        // 5. 哈希新密码
        let new_hash = self
            .password_hasher
            .hash(new_password)
            .map_err(|e| AuthError::PasswordHashError(e.to_string()))?;

        // 6. 记录密码历史
        self.password_history
            .record(user_id, &new_hash)
            .await
            .map_err(|e| AuthError::Internal(e.to_string()))?;

        // 7. 持久化新密码哈希
        self.user_query
            .update_password_hash(user_id, &new_hash)
            .await
            .map_err(|e| AuthError::Internal(e.to_string()))?;

        // 5.3: 修改密码后强制下线用户所有旧会话
        self.revoke_all_tokens(user_id).await?;

        self.audit_token_event(
            "password_changed",
            user_id,
            "",
            "password_changed_all_sessions_revoked",
        )
        .await;
        info!(user_id = user_id, "密码修改成功，已强制下线所有旧会话");
        // 4.5: 审计日志
        self.audit_log(
            "change_password",
            cmx_audit::OperationResult::Success,
            user_id,
            Some("user"),
            Some(user_id),
            None,
        )
        .await;
        Ok(())
    }
}
