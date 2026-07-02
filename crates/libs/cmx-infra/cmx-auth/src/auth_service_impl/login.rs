//! 登录认证
//!
//! 实现 [`cmx_traits::auth::AuthService::authenticate`]（凭据分派）与
//! [`cmx_traits::auth::AuthService::verify_credentials`]，以及内部辅助方法
//! （密码认证、登录失败计数、账号锁定）。

use std::time::Duration;

use cmx_traits::auth::{AuthError, Credentials, DeviceInfo, TokenPair};
use tracing::{info, warn};

use crate::auth_service_impl::AuthServiceImpl;
use crate::metrics;

impl AuthServiceImpl {
    /// 用户名密码认证
    pub(super) async fn authenticate_password(
        &self,
        username: &str,
        password: &str,
        device_info: Option<&DeviceInfo>,
    ) -> std::result::Result<TokenPair, AuthError> {
        // 1. 检查账号锁定
        let lock_key = format!("auth:{{{}}}:locked", username);
        let locked = self
            .cache
            .ops()
            .exists(&lock_key)
            .await
            .map_err(|e| AuthError::Internal(e.to_string()))?;
        if locked {
            let ttl = self
                .cache
                .ttl()
                .ttl(&lock_key)
                .await
                .map_err(|e| AuthError::Internal(e.to_string()))?;
            let secs = ttl.map(|d| d.as_secs()).unwrap_or(0);
            return Err(AuthError::TooManyAttempts {
                secs,
                limit: self.config.cache.max_login_attempts,
                window: self.config.cache.lock_duration_secs,
            });
        }

        // 2. 查询用户
        let user = match self.user_query.get_user_by_username(username).await {
            Ok(Some(u)) => u,
            Ok(None) => {
                // P1-6.7 时序攻击防护：用户不存在时也执行 Argon2 dummy verify
                // 消除响应时间差异，防止攻击者通过时间判断用户是否存在
                let _ = self.password_hasher.verify(
                    password,
                    "$argon2id$v=19$m=65536,t=3,p=4$dummynoncesalt$dummyhash",
                );
                return Err(AuthError::InvalidCredentials);
            }
            Err(e) => return Err(AuthError::Internal(e.to_string())),
        };

        // 3. 检查用户状态
        if user.status == 0 {
            return Err(AuthError::UserDisabled);
        }

        // 4. 校验密码
        let password_hash = user
            .password_hash
            .as_ref()
            .ok_or(AuthError::InvalidCredentials)?;
        let valid = self
            .password_hasher
            .verify(password, password_hash)
            .map_err(|_| AuthError::PasswordVerifyFailed)?;
        if !valid {
            // 记录失败次数
            self.record_login_failure(username).await;
            // 4.5: 审计日志
            self.audit_log(
                "login",
                cmx_audit::OperationResult::Failure,
                username,
                Some("user"),
                Some(username),
                Some(serde_json::json!({"reason": "invalid_credentials"})),
            )
            .await;
            return Err(AuthError::InvalidCredentials);
        }

        // 5. 清除失败计数
        self.clear_login_failures(username).await;

        // 6. 获取角色和权限
        let roles = self
            .user_query
            .get_user_role_codes(&user.user_id)
            .await
            .map_err(|e| AuthError::Internal(e.to_string()))?;
        let permissions = self
            .user_query
            .get_user_permissions(&user.user_id)
            .await
            .map_err(|e| AuthError::Internal(e.to_string()))?;

        // 7. 签发 Token
        let result = self
            .issue_token_pair(
                &user.user_id,
                &user.username,
                &roles,
                &permissions,
                None,
                device_info,
            )
            .await;

        if result.is_ok() {
            metrics::record_login_success("password");
        }

        result
    }

    /// 记录登录失败次数（N9：每次 incr 后都 expire，幂等安全）
    pub(super) async fn record_login_failure(&self, username: &str) {
        let fail_key = format!("auth:{{{}}}:login_fail", username);
        let lock_key = format!("auth:{{{}}}:locked", username);

        if let Ok(count) = self.cache.ops().incr(&fail_key, 1).await {
            // 5.2 修复：expire 失败时 warn 而非静默吞没
            // 6.1 修复：使用 config.cache.lock_duration_secs 而非硬编码 900
            if let Err(e) = self
                .cache
                .ttl()
                .expire(
                    &fail_key,
                    Duration::from_secs(self.config.cache.lock_duration_secs),
                )
                .await
            {
                warn!(key = %fail_key, error = %e, "设置登录失败计数 TTL 失败，key 可能变为永久 key");
            }

            // 达到阈值则锁定
            if count >= self.config.cache.max_login_attempts as i64 {
                if let Err(e) = self
                    .cache
                    .ttl()
                    .set_with_ttl(
                        &lock_key,
                        "1",
                        Duration::from_secs(self.config.cache.lock_duration_secs),
                    )
                    .await
                {
                    warn!(key = %lock_key, error = %e, "设置账号锁定失败");
                }
                warn!(
                    username = username,
                    count = count,
                    "账号已锁定 {} 秒",
                    self.config.cache.lock_duration_secs
                );
            }
        }
    }

    /// 清除登录失败计数
    pub(super) async fn clear_login_failures(&self, username: &str) {
        let fail_key = format!("auth:{{{}}}:login_fail", username);
        // 5.3 修复：del 失败时 warn
        if let Err(e) = self.cache.ops().del(&fail_key).await {
            warn!(key = %fail_key, error = %e, "清除登录失败计数失败，用户可能仍被锁定");
        }
    }

    /// 用户认证入口。
    ///
    /// 根据 `Credentials` 变体分发到不同认证路径：
    /// - `Password` — 用户名密码登录
    /// - `RefreshToken` — 刷新 Access Token
    /// - `ApiKey` — API Key 认证（会创建完整会话，不推荐中间件场景）
    /// - `AuthorizationCode` — OAuth2 授权码换 Token
    /// - `ThirdPartyOAuth2` — 第三方 OAuth2 登录
    ///
    /// # Arguments
    ///
    /// * `credentials` - 各类凭据的统一枚举。
    /// * `device_info` - 设备信息（可选，用于会话创建与限流）。
    ///
    /// # Returns
    ///
    /// 成功时返回 `TokenPair`（Access Token + Refresh Token）。
    ///
    /// # Errors
    ///
    /// 详见 `AuthError` 各变体（账号锁定、用户禁用、密码错误、Token 撤销、PKCE 失败等）。
    pub(super) async fn authenticate(
        &self,
        credentials: Credentials,
        device_info: Option<DeviceInfo>,
    ) -> std::result::Result<TokenPair, AuthError> {
        match credentials {
            Credentials::Password { username, password } => {
                let span = tracing::span!(tracing::Level::INFO, "auth_login", username = %username);
                let _enter = span.enter();
                info!(username = %username, "用户认证开始");
                let result = self
                    .authenticate_password(&username, &password, device_info.as_ref())
                    .await;
                if result.is_ok() {
                    info!(username = %username, "用户认证成功");
                } else {
                    metrics::record_login_failure("invalid_credentials");
                }
                result
            }
            Credentials::RefreshToken { refresh_token } => self.refresh_token(&refresh_token).await,
            Credentials::ApiKey { key } => {
                // 注意：此路径会创建完整会话，不推荐用于中间件高频认证场景。
                // 中间件场景请使用 validate_api_key()（无状态，不创建会话）。
                let api_key_entity = self.validate_api_key_entity(&key).await?;

                // 查询关联用户信息
                let user_id = api_key_entity.user_id.ok_or(AuthError::InvalidApiKey)?;

                let user = self
                    .user_query
                    .get_user_by_id(&user_id)
                    .await
                    .map_err(|e| AuthError::Internal(e.to_string()))?
                    .ok_or(AuthError::InvalidToken("用户不存在".to_string()))?;

                let roles = self
                    .user_query
                    .get_user_role_codes(&user_id)
                    .await
                    .map_err(|e| AuthError::Internal(e.to_string()))?;
                let permissions = self
                    .user_query
                    .get_user_permissions(&user_id)
                    .await
                    .map_err(|e| AuthError::Internal(e.to_string()))?;

                metrics::record_api_key_validation();
                self.issue_token_pair(
                    &user_id,
                    &user.username,
                    &roles,
                    &permissions,
                    None,
                    Some(&DeviceInfo {
                        device_type: "api_key".to_string(),
                        device_id: api_key_entity.key_prefix.clone(),
                        ip: None,
                        user_agent: None,
                    }),
                )
                .await
            }
            Credentials::AuthorizationCode {
                code,
                code_verifier,
                client_id,
            } => {
                let span = tracing::span!(tracing::Level::INFO, "auth_oauth2", code = %code);
                let _enter = span.enter();
                info!(client_id = %client_id, "OAuth2 授权码认证开始");

                // 用授权码换取用户 ID 和 scope
                let (user_id, _scope) = self
                    .oauth2_policy
                    .authenticate(&code, &code_verifier, &client_id, "")
                    .await?;

                // 查询用户信息
                let user = self
                    .user_query
                    .get_user_by_id(&user_id)
                    .await
                    .map_err(|e| AuthError::Internal(e.to_string()))?
                    .ok_or(AuthError::InvalidToken("用户不存在".to_string()))?;

                // 检查用户状态
                if user.status == 0 {
                    return Err(AuthError::UserDisabled);
                }

                // 获取角色和权限
                let roles = self
                    .user_query
                    .get_user_role_codes(&user_id)
                    .await
                    .map_err(|e| AuthError::Internal(e.to_string()))?;
                let permissions = self
                    .user_query
                    .get_user_permissions(&user_id)
                    .await
                    .map_err(|e| AuthError::Internal(e.to_string()))?;

                // 签发 Token 对
                let result = self
                    .issue_token_pair(
                        &user_id,
                        &user.username,
                        &roles,
                        &permissions,
                        None,
                        device_info.as_ref(),
                    )
                    .await;

                if result.is_ok() {
                    info!(user_id = %user_id, "OAuth2 授权码认证成功");
                }
                result
            }
            Credentials::ThirdPartyOAuth2 {
                provider,
                provider_user_id,
                user_id,
            } => {
                let span = tracing::span!(tracing::Level::INFO, "auth_third_party_oauth2", provider = %provider);
                let _enter = span.enter();
                info!(provider = %provider, user_id = %user_id, "第三方 OAuth2 登录");
                self.authenticate_third_party(&user_id, &provider, &provider_user_id, device_info)
                    .await
            }
        }
    }

    /// 仅验证用户名密码，返回 `user_id`（不签发 Token）。
    ///
    /// 用于 OAuth2 `login` 阶段：用户认证后由流程服务签发授权码，
    /// 而非直接签发 Token。包含账号锁定检查、时序攻击防护与审计日志。
    ///
    /// # Arguments
    ///
    /// * `username` - 用户名。
    /// * `password` - 明文密码。
    ///
    /// # Returns
    ///
    /// 成功时返回用户 ID。
    ///
    /// # Errors
    ///
    /// * `AuthError::TooManyAttempts` - 账号已锁定。
    /// * `AuthError::InvalidCredentials` - 用户不存在或密码错误。
    /// * `AuthError::UserDisabled` - 用户已禁用。
    pub(super) async fn verify_credentials(
        &self,
        username: &str,
        password: &str,
    ) -> std::result::Result<String, AuthError> {
        // 1. 检查账号锁定
        let lock_key = format!("auth:{{{}}}:locked", username);
        let locked = self
            .cache
            .ops()
            .exists(&lock_key)
            .await
            .map_err(|e| AuthError::Internal(e.to_string()))?;
        if locked {
            let ttl = self
                .cache
                .ttl()
                .ttl(&lock_key)
                .await
                .map_err(|e| AuthError::Internal(e.to_string()))?;
            let secs = ttl.map(|d| d.as_secs()).unwrap_or(0);
            return Err(AuthError::TooManyAttempts {
                secs,
                limit: self.config.cache.max_login_attempts,
                window: self.config.cache.lock_duration_secs,
            });
        }

        // 2. 查询用户
        let user = match self.user_query.get_user_by_username(username).await {
            Ok(Some(u)) => u,
            Ok(None) => {
                // 时序攻击防护
                let _ = self.password_hasher.verify(
                    password,
                    "$argon2id$v=19$m=65536,t=3,p=4$dummynoncesalt$dummyhash",
                );
                return Err(AuthError::InvalidCredentials);
            }
            Err(e) => return Err(AuthError::Internal(e.to_string())),
        };

        // 3. 检查用户状态
        if user.status == 0 {
            return Err(AuthError::UserDisabled);
        }

        // 4. 校验密码
        let password_hash = user
            .password_hash
            .as_ref()
            .ok_or(AuthError::InvalidCredentials)?;
        let valid = self
            .password_hasher
            .verify(password, password_hash)
            .map_err(|_| AuthError::PasswordVerifyFailed)?;
        if !valid {
            self.record_login_failure(username).await;
            self.audit_log(
                "login",
                cmx_audit::OperationResult::Failure,
                username,
                Some("user"),
                Some(username),
                Some(serde_json::json!({"reason": "invalid_credentials"})),
            )
            .await;
            return Err(AuthError::InvalidCredentials);
        }

        // 5. 清除失败计数
        self.clear_login_failures(username).await;

        // 2.3 修复：记录登录成功 metrics 和审计日志
        metrics::record_login_success("oauth2_verify");
        self.audit_log(
            "verify_credentials",
            cmx_audit::OperationResult::Success,
            &user.user_id,
            Some("user"),
            Some(username),
            None,
        )
        .await;

        Ok(user.user_id)
    }
}
