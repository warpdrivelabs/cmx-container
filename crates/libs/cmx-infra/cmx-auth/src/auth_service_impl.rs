//! AuthService trait 实现
//!
//! 整合 JwtManager、Argon2Hasher、TokenManager、SessionManager，
//! 提供 authenticate / validate_token / refresh_token / revoke_token 等完整认证流程。

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use cmx_buffer::CacheManager;
use cmx_core::AuthContext;
use cmx_traits::auth::{
    AuthError, AuthService, AuthStorageQuery, Credentials, DeviceInfo, OAuth2CallbackExchangeResult,
    OAuth2CallbackResult, OAuth2ClientData, TokenPair, UserAuthQuery, UserInfo,
};
use tracing::{debug, info, warn};
use cmx_utils::snowflake_id_str;
use crate::api_key::ApiKeyEntity;
use crate::config::AuthConfig;
use crate::error::Result;
use crate::jwt::JwtManager;
use crate::metrics;
use crate::oauth2::provider::AccountLinker;
use crate::password::{Argon2Hasher, PasswordHistory, PasswordPolicy};
use crate::policy::OAuth2Policy;
use crate::session::{SessionManager, UserSession};
use crate::token::TokenManager;

/// Pub/Sub 频道名。
const CACHE_INVALIDATE_CHANNEL: &str = "auth:cache:invalidate";

/// API Key AuthContext 缓存 TTL（秒）。
const API_KEY_CTX_CACHE_TTL_SECS: u64 = 60;

/// 回调授权码存储数据（包含 TokenPair + is_new + provider + state）。
#[derive(serde::Serialize, serde::Deserialize)]
struct CallbackCodeData {
    access_token: String,
    refresh_token: String,
    token_type: String,
    access_expires_at: i64,
    refresh_expires_at: i64,
    is_new: bool,
    provider: String,
    state: String,
}

/// `AuthService` trait 的默认实现。
///
/// 整合 `JwtManager`、`Argon2Hasher`、`TokenManager`、`SessionManager`、
/// `OAuth2Policy` 等子模块，提供完整的认证流程实现。
pub struct AuthServiceImpl {
    /// 缓存管理器（用于登录失败计数、账号锁定和 Pub/Sub）。
    cache: CacheManager,
    /// JWT 管理器。
    jwt_manager: JwtManager,
    /// 密码哈希器。
    password_hasher: Argon2Hasher,
    /// Token 管理器。
    token_manager: TokenManager,
    /// 会话管理器。
    session_manager: SessionManager,
    /// OAuth2 策略。
    oauth2_policy: OAuth2Policy,
    /// 密码策略校验器。
    password_policy: PasswordPolicy,
    /// 密码历史校验器。
    password_history: PasswordHistory,
    /// 用户数据查询（由 cmx-biz 实现）。
    user_query: Arc<dyn UserAuthQuery>,
    /// 审计日志记录器。
    audit_logger: Option<Arc<dyn cmx_audit::AuditLogger>>,
    /// 认证配置。
    config: AuthConfig,
    /// OAuth2 存储（用于第三方 Provider state/callback code）。
    oauth2_store: crate::oauth2::OAuth2Store,
    /// 第三方账号关联器。
    account_linker: AccountLinker,
}

impl AuthServiceImpl {
    /// 创建新的 `AuthServiceImpl` 实例。
    ///
    /// 初始化 JWT、密码哈希、Token、会话、OAuth2 等子模块，
    /// 并根据配置构造第三方账号关联器。
    ///
    /// # Arguments
    ///
    /// * `cache` - Redis 缓存管理器，用于登录失败计数、账号锁定和 Pub/Sub。
    /// * `config` - 认证配置（包含 JWT、Token、Argon2、会话、OAuth2 等子配置）。
    /// * `user_query` - 用户数据查询 trait 对象（由 cmx-biz 实现）。
    ///
    /// # Returns
    ///
    /// 成功时返回构造完成的 `AuthServiceImpl` 实例。
    ///
    /// # Errors
    ///
    /// 当 JWT 密钥加载失败或 Argon2 参数无效时返回 `AuthInfraError`。
    pub fn new(
        cache: CacheManager,
        config: AuthConfig,
        user_query: Arc<dyn UserAuthQuery>,
    ) -> Result<Self> {
        let jwt_manager = JwtManager::new(config.clone())?;
        let password_hasher = Argon2Hasher::new(&config.argon2)?;
        let token_manager = TokenManager::new(cache.clone(), config.clone());
        let session_manager = SessionManager::new(cache.clone(), config.clone());
        let oauth2_policy = OAuth2Policy::new(cache.clone(), config.clone());
        let password_policy = PasswordPolicy::new();
        let password_history = PasswordHistory::new(cache.clone(), password_hasher.clone());
        let oauth2_store = crate::oauth2::OAuth2Store::new(cache.clone(), config.clone());
        let account_link_config = config.oauth2
            .as_ref()
            .map(|c| c.account_link.clone())
            .unwrap_or_default();
        let account_linker = AccountLinker::new(user_query.clone(), account_link_config);

        Ok(Self {
            cache,
            jwt_manager,
            password_hasher,
            token_manager,
            session_manager,
            oauth2_policy,
            password_policy,
            password_history,
            user_query,
            audit_logger: None,
            config,
            oauth2_store,
            account_linker,
        })
    }

    /// 设置审计日志记录器。
    ///
    /// 采用 builder 模式，便于在构造时链式注入审计日志实现。
    ///
    /// # Arguments
    ///
    /// * `logger` - 审计日志记录器 trait 对象。
    ///
    /// # Returns
    ///
    /// 返回注入审计日志后的 `Self`，便于链式调用。
    pub fn with_audit_logger(mut self, logger: Arc<dyn cmx_audit::AuditLogger>) -> Self {
        self.audit_logger = Some(logger);
        self
    }

    /// 获取 OAuth2 策略引用。
    ///
    /// # Returns
    ///
    /// 返回内部 `OAuth2Policy` 的引用，供外部调用 authorize/login 等流程。
    pub fn oauth2_policy(&self) -> &OAuth2Policy {
        &self.oauth2_policy
    }

    /// 用户名密码认证
    async fn authenticate_password(
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
                let _ = self.password_hasher.verify(password, "$argon2id$v=19$m=65536,t=3,p=4$dummynoncesalt$dummyhash");
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
            self.audit_log("login", cmx_audit::OperationResult::Failure, username, Some("user"), Some(username), Some(serde_json::json!({"reason": "invalid_credentials"}))).await;
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

    /// 签发 Token 对
    async fn issue_token_pair(
        &self,
        user_id: &str,
        username: &str,
        roles: &[String],
        permissions: &[String],
        org_id: Option<&str>,
        device_info: Option<&DeviceInfo>,
    ) -> std::result::Result<TokenPair, AuthError> {
        let device_type = device_info
            .map(|d| d.device_type.clone())
            .unwrap_or_else(|| "unknown".to_string());
        let session_id = uuid::Uuid::new_v4().to_string();

        // P1-3.7: single_session_per_device_type 互踢检查
        if self.config.session.single_session_per_device_type {
            // 同设备类型互踢：销毁旧会话（HSET 天然覆盖，这里先查后删以记录日志）
            if let Ok(Some(_old_session)) = self
                .session_manager
                .get_session(user_id, &device_type)
                .await
            {
                info!(user_id = user_id, device = %device_type, "互踢: 同设备类型旧会话已覆盖");
            }
        }

        // P1-3.7: max_sessions 并发会话数检查
        let max_sessions = self.config.session.max_sessions;
        if max_sessions > 0 {
            let devices = self
                .session_manager
                .get_user_devices(user_id)
                .await
                .map_err(|e| AuthError::Internal(e.to_string()))?;
            if devices.len() >= max_sessions {
                // 踢掉最早的会话
                if let Some(oldest_device) = devices.first() {
                    self.session_manager
                        .destroy_session(user_id, oldest_device)
                        .await
                        .map_err(|e| AuthError::Internal(e.to_string()))?;
                    info!(
                        user_id = user_id,
                        max = max_sessions,
                        kicked_device = %oldest_device,
                        "max_sessions 超限，踢掉最早会话"
                    );
                }
            }
        }

        // 签发 Access Token
        let access_token = self
            .jwt_manager
            .encode_access_token(
                user_id,
                username,
                roles,
                permissions,
                org_id,
                &session_id,
                &device_type,
            )
            .map_err(|e| AuthError::InvalidToken(e.to_string()))?;

        // 签发 Refresh Token
        let refresh_token = self
            .jwt_manager
            .encode_refresh_token(user_id, &session_id, &device_type)
            .map_err(|e| AuthError::InvalidToken(e.to_string()))?;

        // 解析 Access Claims 获取 jti
        let access_claims = self
            .jwt_manager
            .decode_access_token(&access_token)
            .map_err(|e| AuthError::InvalidToken(e.to_string()))?;

        // 解析 Refresh Claims 获取 jti
        let refresh_claims = self
            .jwt_manager
            .decode_refresh_token(&refresh_token)
            .map_err(|e| AuthError::InvalidToken(e.to_string()))?;

        // 存储 Refresh Token
        self.token_manager
            .store_refresh_token(user_id, &refresh_claims.jti, &device_type)
            .await
            .map_err(|e| AuthError::Internal(e.to_string()))?;

        // 创建会话
        let now = Utc::now().timestamp();
        let session = UserSession {
            session_id: session_id.clone(),
            user_id: user_id.to_string(),
            device_type: device_type.clone(),
            device_id: device_info
                .map(|d| d.device_id.clone())
                .unwrap_or_default(),
            login_at: now,
            last_active_at: now,
            ip: device_info.and_then(|d| d.ip.clone()),
            user_agent: device_info.and_then(|d| d.user_agent.clone()),
            access_jti: access_claims.jti,
            refresh_jti: refresh_claims.jti,
            access_expires_at: now + self.config.token.access_ttl_secs as i64,
        };
        self.session_manager
            .create_session(&session)
            .await
            .map_err(|e| AuthError::Internal(e.to_string()))?;

        // 审计：Token 签发
        self.audit_token_event("token_issued", user_id, &session.access_jti, "access_token_issued").await;

        let now_ts = Utc::now().timestamp();
        Ok(TokenPair {
            access_token,
            refresh_token,
            access_expires_at: now_ts + self.config.token.access_ttl_secs as i64,
            refresh_expires_at: now_ts + self.config.token.refresh_ttl_secs as i64,
            token_type: "Bearer".to_string(),
        })
    }

    /// 记录登录失败次数（N9：每次 incr 后都 expire，幂等安全）
    async fn record_login_failure(&self, username: &str) {
        let fail_key = format!("auth:{{{}}}:login_fail", username);
        let lock_key = format!("auth:{{{}}}:locked", username);

        if let Ok(count) = self.cache.ops().incr(&fail_key, 1).await {
            // 5.2 修复：expire 失败时 warn 而非静默吞没
            // 6.1 修复：使用 config.cache.lock_duration_secs 而非硬编码 900
            if let Err(e) = self
                .cache
                .ttl()
                .expire(&fail_key, Duration::from_secs(self.config.cache.lock_duration_secs))
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
    async fn clear_login_failures(&self, username: &str) {
        let fail_key = format!("auth:{{{}}}:login_fail", username);
        // 5.3 修复：del 失败时 warn
        if let Err(e) = self.cache.ops().del(&fail_key).await {
            warn!(key = %fail_key, error = %e, "清除登录失败计数失败，用户可能仍被锁定");
        }
    }

    /// P0-2.3: 发布 Pub/Sub 缓存失效消息
    async fn publish_cache_invalidate(&self, message: &str) {
        if let Err(e) = self.cache.pubsub().publish(CACHE_INVALIDATE_CHANNEL, message).await {
            warn!(channel = CACHE_INVALIDATE_CHANNEL, error = %e, "Pub/Sub 缓存失效消息发布失败");
        }
    }

    /// 4.5: 记录审计日志
    async fn audit_log(
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

    /// 记录 Token 审计日志
    async fn audit_token_event(&self, event_type: &str, user_id: &str, jti: &str, detail: &str) {
        if let Err(e) = AuthStorageQuery::record_token_event(self, event_type, user_id, jti, detail)
            .await
        {
            warn!(event_type = event_type, user_id = user_id, error = %e, "审计日志写入失败");
        }
    }

    /// 验证 API Key 并返回实体（直接调用 AuthStorageQuery，无需外部 trait 对象）
    async fn validate_api_key_entity(
        &self,
        api_key: &str,
    ) -> std::result::Result<ApiKeyEntity, AuthError> {
        // 1. 提取 key_prefix（格式：cmx_xxxxxxxx...）
        let key_prefix = if api_key.len() >= 8 {
            &api_key[..8]
        } else {
            return Err(AuthError::InvalidApiKey);
        };

        // 2. 通过 AuthStorageQuery 查询 API Key
        let api_key_data = AuthStorageQuery::get_api_key_by_prefix(self, key_prefix)
            .await
            .map_err(|e| AuthError::Internal(e.to_string()))?
            .ok_or(AuthError::InvalidApiKey)?;

        // 3. 检查状态
        if api_key_data.status == 0 {
            return Err(AuthError::InvalidApiKey);
        }

        // 4. 使用 SHA256 验证 key
        let input_hash = {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(api_key.as_bytes());
            hex::encode(hasher.finalize())
        };
        if input_hash != api_key_data.key_hash {
            return Err(AuthError::InvalidApiKey);
        }

        Ok(ApiKeyEntity {
            key_prefix: api_key_data.key_prefix,
            key_hash: api_key_data.key_hash,
            user_id: api_key_data.user_id,
            service_name: api_key_data.service_name,
            scopes: api_key_data.scopes,
            description: api_key_data.description,
            status: api_key_data.status,
        })
    }

    /// 第三方 OAuth2 登录认证
    async fn authenticate_third_party(
        &self,
        user_id: &str,
        provider: &str,
        provider_user_id: &str,
        device_info: Option<DeviceInfo>,
    ) -> std::result::Result<TokenPair, AuthError> {
        // 1. 验证用户存在且启用
        let user = self.user_query.get_user_by_id(user_id).await
            .map_err(|e| AuthError::Internal(e.to_string()))?
            .ok_or(AuthError::OAuth2AccountNotLinked {
                provider: provider.to_string(),
                provider_user_id: provider_user_id.to_string(),
            })?;

        if user.status == 0 {
            return Err(AuthError::UserDisabled);
        }

        // 2. 获取角色和权限
        let roles = self.user_query.get_user_role_codes(user_id).await
            .map_err(|e| AuthError::Internal(e.to_string()))?;
        let permissions = self.user_query.get_user_permissions(user_id).await
            .map_err(|e| AuthError::Internal(e.to_string()))?;

        // 3. 签发 TokenPair（org_id 传 None，与现有密码/APIKey/OAuth2 分支一致）
        let result = self.issue_token_pair(
            user_id, &user.username, &roles, &permissions,
            None,
            device_info.as_ref(),
        ).await;

        if result.is_ok() {
            metrics::record_login_success("third_party_oauth2");
            info!(provider = %provider, user_id = %user_id, "第三方 OAuth2 登录成功");
        }

        result
    }
}

#[async_trait]
impl AuthService for AuthServiceImpl {
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
    async fn authenticate(
        &self,
        credentials: Credentials,
        device_info: Option<DeviceInfo>,
    ) -> std::result::Result<TokenPair, AuthError> {
        match credentials {
            Credentials::Password { username, password } => {
                let span =
                    tracing::span!(tracing::Level::INFO, "auth_login", username = %username);
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
            Credentials::RefreshToken { refresh_token } => {
                self.refresh_token(&refresh_token).await
            }
            Credentials::ApiKey { key } => {
                // 注意：此路径会创建完整会话，不推荐用于中间件高频认证场景。
                // 中间件场景请使用 validate_api_key()（无状态，不创建会话）。
                let api_key_entity = self.validate_api_key_entity(&key).await?;

                // 查询关联用户信息
                let user_id = api_key_entity
                    .user_id
                    .ok_or(AuthError::InvalidApiKey)?;

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
            Credentials::ThirdPartyOAuth2 { provider, provider_user_id, user_id } => {
                let span = tracing::span!(tracing::Level::INFO, "auth_third_party_oauth2", provider = %provider);
                let _enter = span.enter();
                info!(provider = %provider, user_id = %user_id, "第三方 OAuth2 登录");
                self.authenticate_third_party(&user_id, &provider, &provider_user_id, device_info).await
            }
        }
    }

    /// 验证 Access Token 并返回 `AuthContext`。
    ///
    /// 解码 Token → 检查黑名单 → 检查会话活跃度，并记录验证耗时指标。
    ///
    /// # Arguments
    ///
    /// * `token` - 待验证的 Access Token 字符串。
    ///
    /// # Returns
    ///
    /// 包含用户身份信息的 `AuthContext`。
    ///
    /// # Errors
    ///
    /// * `AuthError::InvalidToken` - Token 解析失败或过期。
    /// * `AuthError::TokenRevoked` - Token 已被加入黑名单。
    /// * `AuthError::SessionNotFound` - 会话不存在或不活跃。
    async fn validate_token(&self, token: &str) -> std::result::Result<AuthContext, AuthError> {
        // 2.1 修复：记录 Token 验证耗时
        let start = std::time::Instant::now();

        // 1. 解码 Token
        let claims = self
            .jwt_manager
            .decode_access_token(token)
            .map_err(|e| AuthError::InvalidToken(e.to_string()))?;

        // 2. 检查黑名单
        if self
            .token_manager
            .is_blacklisted(&claims.jti)
            .await
            .map_err(|e| AuthError::Internal(e.to_string()))?
        {
            return Err(AuthError::TokenRevoked);
        }

        // 3. 检查会话是否活跃
        if !self
            .session_manager
            .is_session_active(&claims.sub, &claims.device)
            .await
            .map_err(|e| AuthError::Internal(e.to_string()))?
        {
            return Err(AuthError::SessionNotFound);
        }

        // 4. 构建 AuthContext
        let elapsed = start.elapsed().as_secs_f64();
        metrics::record_validate_duration("jwt_bearer", elapsed);

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

    /// 用 Refresh Token 换发新 Token 对（Rotation 防重放）。
    ///
    /// 使用 Lua 脚本原子执行"检查旧 jti → 删除旧 token"，失败时视为重放攻击。
    /// 成功后重新签发 Access + Refresh Token 对。
    ///
    /// # Arguments
    ///
    /// * `refresh_token` - 待刷新的 Refresh Token 字符串。
    ///
    /// # Returns
    ///
    /// 新的 `TokenPair`。
    ///
    /// # Errors
    ///
    /// * `AuthError::InvalidToken` - Token 解析失败。
    /// * `AuthError::ReplayDetected` - 旧 jti 不存在（重放攻击）。
    /// * `AuthError::UserDisabled` - 用户已禁用。
    async fn refresh_token(
        &self,
        refresh_token: &str,
    ) -> std::result::Result<TokenPair, AuthError> {
        // 1. 解码 Refresh Token
        let claims = self
            .jwt_manager
            .decode_refresh_token(refresh_token)
            .map_err(|e| AuthError::InvalidToken(e.to_string()))?;

        // 2. Lua 原子操作：检查旧 jti 是否存在 + 删除旧 token
        let rotated = self
            .token_manager
            .rotation()
            .rotate_refresh_token(&claims.sub, &claims.jti)
            .await
            .map_err(|e| AuthError::Internal(e.to_string()))?;

        if !rotated {
            return Err(AuthError::ReplayDetected);
        }

        // 3. 通过 user_id 查询用户信息
        let user = self
            .user_query
            .get_user_by_id(&claims.sub)
            .await
            .map_err(|e| AuthError::Internal(e.to_string()))?
            .ok_or(AuthError::InvalidToken("用户不存在".to_string()))?;

        // 4. 检查用户状态
        if user.status == 0 {
            return Err(AuthError::UserDisabled);
        }

        // 5. 重新获取角色和权限
        let roles = self
            .user_query
            .get_user_role_codes(&claims.sub)
            .await
            .map_err(|e| AuthError::Internal(e.to_string()))?;
        let permissions = self
            .user_query
            .get_user_permissions(&claims.sub)
            .await
            .map_err(|e| AuthError::Internal(e.to_string()))?;

        // 6. 签发新 Token 对（store_refresh_token 在内部完成新 token 的创建）
        let device_info = DeviceInfo {
            device_type: claims.device.clone(),
            device_id: String::new(),
            ip: None,
            user_agent: None,
        };
        self.issue_token_pair(
            &claims.sub,
            &user.username,
            &roles,
            &permissions,
            None,
            Some(&device_info),
        )
        .await
    }

    /// 撤销指定 Token。
    ///
    /// 自动识别 Access Token（加入黑名单）或 Refresh Token（删除记录），
    /// 并通过 Pub/Sub 广播本地缓存失效。
    ///
    /// # Arguments
    ///
    /// * `token` - 待撤销的 Token 字符串（Access 或 Refresh）。
    ///
    /// # Errors
    ///
    /// * `AuthError::InvalidToken` - Token 无法解码为已知类型。
    async fn revoke_token(&self, token: &str) -> std::result::Result<(), AuthError> {
        // 尝试解码为 Access Token
        if let Ok(claims) = self.jwt_manager.decode_access_token(token) {
            let remaining = Duration::from_secs(
                (claims.exp - Utc::now().timestamp()).max(0) as u64,
            );
            self.token_manager
                .blacklist_access_token(&claims.jti, remaining)
                .await
                .map_err(|e| AuthError::Internal(e.to_string()))?;
            // P0-2.3: Pub/Sub 广播本地缓存失效
            self.publish_cache_invalidate(&format!("blacklist:{}", claims.jti)).await;
            info!(jti = %claims.jti, "Access Token 已撤销");
            metrics::record_token_revoked("access");
            self.audit_token_event("token_revoked", &claims.sub, &claims.jti, "single_token_revoked").await;
            return Ok(());
        }

        // 尝试解码为 Refresh Token
        if let Ok(claims) = self.jwt_manager.decode_refresh_token(token) {
            self.token_manager
                .revoke_refresh_token(&claims.sub, &claims.jti)
                .await
                .map_err(|e| AuthError::Internal(e.to_string()))?;
            info!(jti = %claims.jti, "Refresh Token 已撤销");
            metrics::record_token_revoked("refresh");
            self.audit_token_event("token_revoked", &claims.sub, &claims.jti, "single_token_revoked").await;
            return Ok(());
        }

        Err(AuthError::InvalidToken("无法解码 Token".to_string()))
    }

    /// 撤销用户的所有 Token 与会话。
    ///
    /// 撤销所有 Refresh Token + 将所有 Access Token 加入黑名单（TTL 取剩余有效期）+
    /// 销毁所有会话 + Pub/Sub 广播本地缓存失效。
    ///
    /// # Arguments
    ///
    /// * `user_id` - 目标用户 ID。
    ///
    /// # Errors
    ///
    /// * `AuthError::Internal` - Redis 操作失败。
    async fn revoke_all_tokens(&self, user_id: &str) -> std::result::Result<(), AuthError> {
        // 撤销所有 Refresh Token
        self.token_manager
            .revoke_all_refresh_tokens(user_id)
            .await
            .map_err(|e| AuthError::Internal(e.to_string()))?;

        // P1-3.6: 获取每个会话的 access_jti，将 Access Token 加入黑名单
        let devices = self
            .session_manager
            .get_user_devices(user_id)
            .await
            .map_err(|e| AuthError::Internal(e.to_string()))?;
        for device in &devices {
            if let Ok(Some(session)) = self.session_manager.get_session(user_id, device).await {
                // 2.2 修复：使用 Access Token 实际剩余有效期，而非完整 access_ttl_secs
                let remaining_secs = (session.access_expires_at - Utc::now().timestamp()).max(0) as u64;
                let remaining = Duration::from_secs(remaining_secs);
                let _ = self.token_manager
                    .blacklist_access_token(&session.access_jti, remaining)
                    .await;
            }
        }

        // 销毁所有会话
        for device in &devices {
            self.session_manager
                .destroy_session(user_id, device)
                .await
                .map_err(|e| AuthError::Internal(e.to_string()))?;
        }

        // P0-2.3: Pub/Sub 广播全部本地缓存失效
        self.publish_cache_invalidate(&format!("revoke_all:{}", user_id)).await;
        // 本实例也立即失效本地缓存
        self.token_manager.invalidate_local_cache_all().await;

        info!(user_id = user_id, "用户所有 Token 已撤销");
        self.audit_token_event("tokens_revoked", user_id, "", "all_tokens_revoked").await;
        Ok(())
    }

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
    async fn hash_password(&self, plain: &str) -> std::result::Result<String, AuthError> {
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
    async fn verify_password(
        &self,
        plain: &str,
        hash: &str,
    ) -> std::result::Result<bool, AuthError> {
        self.password_hasher
            .verify(plain, hash)
            .map_err(|_| AuthError::PasswordVerifyFailed)
    }

    /// 刷新指定用户/设备的会话心跳。
    ///
    /// 更新会话的 `last_active_at` 字段并刷新 `session_detail` 的 TTL，
    /// 用于维持会话活跃状态。
    ///
    /// # Arguments
    ///
    /// * `user_id` - 目标用户 ID。
    /// * `device_type` - 设备类型。
    ///
    /// # Returns
    ///
    /// 会话存在并刷新成功时返回 `true`，会话不存在返回 `false`。
    ///
    /// # Errors
    ///
    /// 当 Redis 操作失败时返回 `AuthError::Internal`。
    async fn heartbeat(
        &self,
        user_id: &str,
        device_type: &str,
    ) -> std::result::Result<bool, AuthError> {
        self.session_manager
            .heartbeat(user_id, device_type)
            .await
            .map_err(|e| AuthError::Internal(e.to_string()))
    }

    /// 根据 Pub/Sub 消息失效本地缓存。
    ///
    /// 支持的消息前缀：
    /// - `blacklist:{jti}` — 失效指定 Token 的黑名单本地缓存。
    /// - `revoke_all:{user_id}` — 批量失效 Token 与 Session 本地缓存。
    ///
    /// # Arguments
    ///
    /// * `message` - Pub/Sub 推送的缓存失效消息。
    async fn invalidate_local_cache(&self, message: &str) {
        if let Some(jti) = message.strip_prefix("blacklist:") {
            self.token_manager.invalidate_local_cache(jti).await;
            // 使对应 session 的本地缓存也失效
            // blacklist: 消息只含 jti，无法精确定位 user_id，依赖 TTL 自然过期即可
        } else if message.starts_with("revoke_all:") {
            self.token_manager.invalidate_local_cache_all().await;
            // P0-2.2 修复：同时清理 Session 本地缓存
            self.session_manager.invalidate_local_all().await;
        }
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
    async fn change_password(
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

        self.audit_token_event("password_changed", user_id, "", "password_changed_all_sessions_revoked").await;
        info!(user_id = user_id, "密码修改成功，已强制下线所有旧会话");
        // 4.5: 审计日志
        self.audit_log("change_password", cmx_audit::OperationResult::Success, user_id, Some("user"), Some(user_id), None).await;
        Ok(())
    }

    /// 确保超管账号存在并同步密码。
    ///
    /// 启动时调用：若超管不存在则创建，已存在则将密码同步为配置中的值
    /// （配置为密码唯一真源）。配置为 `None` 时跳过。
    ///
    /// # Errors
    ///
    /// * `AuthError::PasswordHashError` - 密码哈希失败。
    /// * `AuthError::Internal` - 用户查询或创建失败。
    async fn ensure_super_admin(&self) -> std::result::Result<(), AuthError> {
        if let Some(ref sa_config) = self.config.super_admin {
            // 1. 检查超管是否已存在
            let existing = self
                .user_query
                .get_user_by_username(&sa_config.username)
                .await
                .map_err(|e| AuthError::Internal(e.to_string()))?;

            // 2. 哈希密码（创建和更新都需要）
            let password_hash = self
                .password_hasher
                .hash(&sa_config.password)
                .map_err(|e| AuthError::PasswordHashError(e.to_string()))?;

            match existing {
                None => {
                    // 3. 不存在 → 创建
                    self.user_query
                        .create_super_admin(
                            &sa_config.username,
                            &password_hash,
                            sa_config.email.as_deref(),
                            &sa_config.roles,
                        )
                        .await
                        .map_err(|e| AuthError::Internal(e.to_string()))?;

                    info!(username = %sa_config.username, "超管账号创建成功");
                }
                Some(user) => {
                    // 4. 已存在 → 同步密码（配置为密码唯一真源）
                    self.user_query
                        .update_password_hash(&user.user_id, &password_hash)
                        .await
                        .map_err(|e| AuthError::Internal(e.to_string()))?;

                    info!(username = %sa_config.username, "超管账号已存在，密码已同步");
                }
            }
        }
        Ok(())
    }

    /// 导入配置文件中的静态 API Key 到数据库。
    ///
    /// 启动时调用：遍历 `static_api_keys` 配置，对明文 Key 计算 SHA256 哈希后
    /// 通过 `AuthStorageQuery::upsert_api_key` 持久化（已存在则覆盖）。
    /// 配置为空时直接返回。
    ///
    /// # Errors
    ///
    /// 当数据库写入失败时返回 `AuthError::Internal`。
    async fn import_static_api_keys(&self) -> std::result::Result<(), AuthError> {
        if self.config.static_api_keys.is_empty() {
            return Ok(());
        }

        for api_key_config in &self.config.static_api_keys {
            // 解析 key_prefix：优先显式配置，否则从 key 前 8 位提取
            let key_prefix = api_key_config.resolve_key_prefix();

            // 2.1 修复：API Key 使用 SHA256 哈希（与 ApiKeyManager::validate 一致）
            let hash = {
                use sha2::{Digest, Sha256};
                let mut hasher = Sha256::new();
                hasher.update(api_key_config.key.as_bytes());
                hex::encode(hasher.finalize())
            };

            // 通过 AuthStorageQuery 持久化
            AuthStorageQuery::upsert_api_key(
                self,
                &key_prefix,
                &hash,
                api_key_config.user_id.as_deref(),
                api_key_config.service_name.as_deref(),
                &api_key_config.scopes,
                api_key_config.description.as_deref(),
            )
            .await
            .map_err(|e| AuthError::Internal(e.to_string()))?;

            // 启动日志清晰打印 Key 信息（仅启动时一次，便于管理员获取）
            // 警告：明文 Key 会输出到日志，请确保日志安全
            info!(
                key_prefix = %key_prefix,
                key = %api_key_config.key,
                service_name = api_key_config.service_name.as_deref().unwrap_or("-"),
                user_id = api_key_config.user_id.as_deref().unwrap_or("-"),
                scopes = ?api_key_config.scopes,
                description = api_key_config.description.as_deref().unwrap_or("-"),
                "静态 API Key 已导入（如已存在则覆盖）；将上面 key 值作为 X-API-Key 请求头"
            );
        }

        Ok(())
    }

    /// 启动后台会话清理任务。
    ///
    /// 周期性扫描在线用户会话，清理超过 `idle_timeout_secs` 的过期会话，
    /// 并同步清理 SessionManager 本地缓存与 `session_detail` Key。
    /// 任务以 `tokio::spawn` 方式运行，间隔取 `heartbeat_interval_secs` 与 300 秒的较大值。
    async fn start_cleanup_task(&self) {
        let cache = self.cache.clone();
        let idle_timeout = self.config.session.idle_timeout_secs;
        let heartbeat_interval = self.config.session.heartbeat_interval_secs;
        // 2.2 修复：持有 SessionManager 引用以清理本地缓存
        let session_manager = self.session_manager.clone();

        tokio::spawn(async move {
            let interval_secs = heartbeat_interval.max(300);
            let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));
            loop {
                interval.tick().await;

                // 获取在线用户列表
                let online_users = match cache.set().smembers("auth:online:users").await {
                    Ok(users) => users,
                    Err(e) => {
                        warn!(error = %e, "获取在线用户列表失败");
                        continue;
                    }
                };

                let now = Utc::now().timestamp();
                for user_id in &online_users {
                    let key = format!("auth:{{{}}}:session", user_id);
                    let devices = match cache.hash().hkeys(&key).await {
                        Ok(d) => d,
                        Err(_) => continue,
                    };
                    let mut all_expired = true;
                    for device in &devices {
                        if let Ok(Some(json)) = cache.hash().hget(&key, device).await
                            && let Ok(session) = serde_json::from_str::<UserSession>(&json) {
                                if now - session.last_active_at > idle_timeout as i64 {
                                    // 过期，删除会话 Hash field
                                    let _ = cache.hash().hdel(&key, &[device]).await;
                                    // 2.3 修复：删除 session_detail Key
                                    let detail_key = format!("auth:{}:session_detail", session.session_id);
                                    let _ = cache.ops().del(&detail_key).await;
                                    // 2.2 修复：清理 SessionManager 本地缓存
                                    session_manager.invalidate_local(user_id, device).await;
                                    info!(user_id = user_id, device = %device, "过期会话已清理");
                                } else {
                                    all_expired = false;
                                }
                            }
                    }
                    // 如果所有会话都过期，从在线用户集合移除
                    if all_expired {
                        let remaining = cache.hash().hlen(&key).await.unwrap_or(0);
                        if remaining == 0 {
                            let _ = cache.set().srem_one("auth:online:users", user_id).await;
                        }
                    }
                }
            }
        });
    }

    /// 查询 OAuth2 客户端信息。
    ///
    /// # Arguments
    ///
    /// * `client_id` - OAuth2 客户端 ID。
    ///
    /// # Returns
    ///
    /// 客户端存在时返回 `Some(OAuth2ClientData)`，否则返回 `None`。
    ///
    /// # Errors
    ///
    /// 当数据库查询失败时返回 `AuthError::Internal`。
    async fn get_oauth2_client(
        &self,
        client_id: &str,
    ) -> std::result::Result<Option<OAuth2ClientData>, AuthError> {
        AuthStorageQuery::get_oauth2_client(self, client_id)
            .await
            .map_err(|e| AuthError::Internal(e.to_string()))
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
    async fn verify_credentials(
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
                let _ = self.password_hasher.verify(password, "$argon2id$v=19$m=65536,t=3,p=4$dummynoncesalt$dummyhash");
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
            self.audit_log("login", cmx_audit::OperationResult::Failure, username, Some("user"), Some(username), Some(serde_json::json!({"reason": "invalid_credentials"}))).await;
            return Err(AuthError::InvalidCredentials);
        }

        // 5. 清除失败计数
        self.clear_login_failures(username).await;

        // 2.3 修复：记录登录成功 metrics 和审计日志
        metrics::record_login_success("oauth2_verify");
        self.audit_log("verify_credentials", cmx_audit::OperationResult::Success, &user.user_id, Some("user"), Some(username), None).await;

        Ok(user.user_id)
    }

    /// 验证 API Key 并返回 `AuthContext`（无状态，不创建会话）。
    ///
    /// # 两层缓存优化
    ///
    /// 为避免高频 M2M 调用打垮数据库，使用两层缓存：
    /// 1. 第一层（ApiKeyManager）：`key_prefix → ApiKeyEntity`，缓存 API Key 元数据。
    /// 2. 第二层（本方法）：`key_prefix → AuthContext`，缓存完整认证上下文（含 user/roles/permissions）。
    ///
    /// 缓存命中时跳过全部 4 次 DB 查询，仅做 SHA256 校验。
    /// 缓存失效通过 `invalidate_local_cache("api_key:{key_prefix}")` 触发。
    ///
    /// # Arguments
    ///
    /// * `key` - 待验证的 API Key 明文字符串。
    ///
    /// # Returns
    ///
    /// 成功时返回包含用户身份信息的 `AuthContext`。
    ///
    /// # Errors
    ///
    /// * `AuthError::InvalidApiKey` - Key 格式错误、状态禁用或哈希不匹配。
    /// * `AuthError::UserDisabled` - 关联用户已禁用。
    async fn validate_api_key(&self, key: &str) -> std::result::Result<AuthContext, AuthError> {
        // 提取 key_prefix 用于缓存查找
        let key_prefix = if key.len() >= 8 { &key[..8] } else { return Err(AuthError::InvalidApiKey); };

        // 2.1 修复：API Key 验证不计入 LOGIN_TOTAL，使用专用指标
        metrics::record_api_key_validation();

        // === 第二层缓存：key_prefix → AuthContext ===
        let ctx_cache_key = format!("auth:api_key_ctx:{}", key_prefix);
        if let Ok(Some(cached)) = self.cache.ops().get(&ctx_cache_key).await
            && let Ok(auth_ctx) = serde_json::from_str::<AuthContext>(&cached) {
                // 缓存命中：仍需校验明文 key 的 SHA256（防止缓存被篡改后绕过校验）
                // validate_api_key_entity 内部走第一层缓存，命中时仅做 SHA256 比对
                self.validate_api_key_entity(key).await?;
                debug!(key_prefix = %key_prefix, "API Key AuthContext 缓存命中，跳过 user/roles/permissions 查询");
                return Ok(auth_ctx);
            }

        // === 缓存未命中，走完整验证流程 ===
        let api_key_entity = self.validate_api_key_entity(key).await?;

        // user_id 为空表示未关联用户（纯服务间调用），不报错，跳过用户/角色/权限查询
        let user_id = api_key_entity.user_id.unwrap_or_default();

        if user_id.is_empty() {
            let auth_ctx = AuthContext {
                user_id: String::new(),
                username: String::new(),
                roles: vec![],
                permissions: vec![],
                org_id: None,
                session_id: None,
                device_type: Some("api_key".to_string()),
                auth_method: Some("api_key".to_string()),
            };
            // 写入缓存
            if let Ok(json) = serde_json::to_string(&auth_ctx) {
                let _ = self.cache.ttl().set_with_ttl(
                    &ctx_cache_key,
                    &json,
                    Duration::from_secs(API_KEY_CTX_CACHE_TTL_SECS),
                ).await;
            }
            return Ok(auth_ctx);
        }

        let user = self
            .user_query
            .get_user_by_id(&user_id)
            .await
            .map_err(|e| AuthError::Internal(e.to_string()))?
            .ok_or(AuthError::InvalidToken("用户不存在".to_string()))?;

        if user.status == 0 {
            return Err(AuthError::UserDisabled);
        }

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

        let auth_ctx = AuthContext {
            user_id: user_id.clone(),
            username: user.username,
            roles,
            permissions,
            org_id: None,
            session_id: None,
            device_type: Some("api_key".to_string()),
            auth_method: Some("api_key".to_string()),
        };

        // 写入第二层缓存（TTL 60 秒）
        if let Ok(json) = serde_json::to_string(&auth_ctx) {
            let _ = self.cache.ttl().set_with_ttl(
                &ctx_cache_key,
                &json,
                Duration::from_secs(API_KEY_CTX_CACHE_TTL_SECS),
            ).await;
            debug!(key_prefix = %key_prefix, "API Key AuthContext 已写入缓存");
        }

        Ok(auth_ctx)
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
    async fn get_user_info(&self, user_id: &str) -> std::result::Result<UserInfo, AuthError> {
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

    /// 列出所有已启用的第三方 OAuth2 Provider 信息。
    ///
    /// # Returns
    ///
    /// 返回 `Vec<ProviderInfo>`。注册表未初始化时返回空列表，
    /// 使公开端点 `GET /api/auth/oauth2/providers` 优雅返回空数组。
    async fn list_oauth2_providers(
        &self,
    ) -> std::result::Result<Vec<cmx_traits::auth::ProviderInfo>, AuthError> {
        // 未配置任何第三方 Provider 时（注册表未初始化），优雅返回空列表而非报错，
        // 使公开端点 GET /api/auth/oauth2/providers 返回 {code:0,data:[]}。
        match crate::oauth2::OAuth2ProviderRegistry::get_global() {
            Some(registry) => Ok(registry.list_providers()),
            None => Ok(Vec::new()),
        }
    }

    /// 处理第三方 OAuth2 回调。
    ///
    /// 流程：原子消费 state → 交换 Token → 获取用户信息 → 关联/注册用户 →
    /// 签发本平台 Token → 存储一次性回调授权码（前端用 code 换取 TokenPair）。
    ///
    /// # Arguments
    ///
    /// * `provider` - 第三方 Provider 名称。
    /// * `code` - 第三方 Provider 返回的授权码。
    /// * `state` - CSRF state（与 `store_oauth2_provider_state` 时存储的对应）。
    /// * `device_info` - 设备信息（可选）。
    ///
    /// # Returns
    ///
    /// 包含一次性 `callback_code` 的 `OAuth2CallbackResult`，前端用它换取 TokenPair。
    ///
    /// # Errors
    ///
    /// * `AuthError::OAuth2` - state 无效/provider 不匹配/账号未注册等。
    /// * `AuthError::OAuth2ProviderUnavailable` - 第三方服务不可达。
    async fn handle_oauth2_callback(
        &self,
        provider: &str,
        code: &str,
        state: &str,
        device_info: Option<DeviceInfo>,
    ) -> std::result::Result<OAuth2CallbackResult, AuthError> {
        // 1. 原子消费 state，获取 provider 名称
        let stored_provider = self.oauth2_store.consume_provider_state(state).await
            .map_err(|e| AuthError::Internal(e.to_string()))?
            .ok_or(AuthError::OAuth2("OAuth2 Provider state 无效或已过期".to_string()))?;

        if stored_provider != provider {
            return Err(AuthError::OAuth2("State 中的 provider 与请求不匹配".to_string()));
        }

        // 2. 获取 Provider
        let registry = crate::oauth2::OAuth2ProviderRegistry::get_global()
            .ok_or(AuthError::Internal("OAuth2 Provider 注册表未初始化".to_string()))?;
        let provider_impl = registry.get_provider(provider)?;

        // 3. 获取 redirect_uri（从 Provider 配置中获取）
        let redirect_uri = provider_impl.redirect_uri().to_string();

        // 4. 交换 Token
        let token_response = provider_impl.exchange_code(code, &redirect_uri).await?;
        tracing::info!(provider = %provider, "Token 交换成功");

        // 5. 获取用户信息
        let user_info = provider_impl.get_user_info(&token_response).await?;
        tracing::info!(provider = %provider, provider_user_id = %user_info.provider_user_id, "用户信息获取成功");

        // 6. 关联/注册用户
        let link_result = self.account_linker.find_or_link(provider, &user_info.provider_user_id, &user_info).await?;

        let (user_id, is_new) = match link_result {
            crate::oauth2::provider::LinkResult::Linked { user_id, is_new } => (user_id, is_new),
            crate::oauth2::provider::LinkResult::BindingRequired { .. } => {
                return Err(AuthError::OAuth2("账号未注册，请联系管理员开通".to_string()));
            }
        };

        // 7. 签发本平台 Token
        let token_pair = self.authenticate(
            Credentials::ThirdPartyOAuth2 {
                provider: provider.to_string(),
                provider_user_id: user_info.provider_user_id,
                user_id: user_id.clone(),
            },
            device_info,
        ).await?;

        // 8. 签发一次性回调授权码（存储 TokenPair + is_new + provider）
        let callback_code = uuid::Uuid::new_v4().to_string();
        let callback_data = CallbackCodeData {
            access_token: token_pair.access_token,
            refresh_token: token_pair.refresh_token,
            token_type: token_pair.token_type,
            access_expires_at: token_pair.access_expires_at,
            refresh_expires_at: token_pair.refresh_expires_at,
            is_new,
            provider: provider.to_string(),
            state: state.to_string(),
        };
        let callback_data_json = serde_json::to_string(&callback_data)
            .map_err(|e| AuthError::Internal(e.to_string()))?;
        self.oauth2_store.store_callback_code(&callback_code, &callback_data_json).await
            .map_err(|e| AuthError::Internal(e.to_string()))?;

        // 审计日志：第三方 OAuth2 登录
        self.audit_log("oauth2_login", cmx_audit::OperationResult::Success, &user_id, Some("user"), Some(&user_id), Some(serde_json::json!({
            "provider": provider,
            "is_new": is_new,
        }))).await;

        Ok(cmx_traits::auth::OAuth2CallbackResult {
            callback_code,
            state: state.to_string(),
            is_new,
            provider: provider.to_string(),
        })
    }

    /// 用一次性回调授权码换取 TokenPair。
    ///
    /// 原子消费回调授权码，返回此前在 `handle_oauth2_callback` 中存储的 TokenPair
    /// 及关联的 `is_new`/`provider`/`state` 元信息。
    ///
    /// # Arguments
    ///
    /// * `code` - 一次性回调授权码。
    ///
    /// # Returns
    ///
    /// 成功时返回 `OAuth2CallbackExchangeResult`，包含 TokenPair 与元信息。
    ///
    /// # Errors
    ///
    /// * `AuthError::OAuth2CallbackCodeInvalid` - 授权码无效或已使用。
    /// * `AuthError::Internal` - 回调数据反序列化失败。
    async fn exchange_oauth2_callback_code(
        &self,
        code: &str,
    ) -> std::result::Result<OAuth2CallbackExchangeResult, AuthError> {
        let json = self.oauth2_store.consume_callback_code(code).await
            .map_err(|e| AuthError::Internal(e.to_string()))?
            .ok_or(AuthError::OAuth2CallbackCodeInvalid)?;

        let callback_data: CallbackCodeData = serde_json::from_str(&json)
            .map_err(|e| AuthError::Internal(format!("回调数据反序列化失败: {}", e)))?;

        Ok(OAuth2CallbackExchangeResult {
            access_token: callback_data.access_token,
            refresh_token: callback_data.refresh_token,
            token_type: callback_data.token_type,
            access_expires_at: callback_data.access_expires_at,
            refresh_expires_at: callback_data.refresh_expires_at,
            is_new: callback_data.is_new,
            provider: callback_data.provider,
            state: callback_data.state,
        })
    }

    /// 将第三方 Provider 账号绑定到已有本地用户。
    ///
    /// 流程：获取 Provider → 交换 Token → 获取用户信息 →
    /// 检查账号未被他人绑定 → 创建关联记录。
    ///
    /// # Arguments
    ///
    /// * `user_id` - 本地用户 ID。
    /// * `provider` - 第三方 Provider 名称。
    /// * `code` - 第三方 Provider 返回的授权码。
    ///
    /// # Errors
    ///
    /// * `AuthError::OAuth2` - 该 Provider 账号已被其他用户绑定。
    /// * `AuthError::OAuth2ProviderUnavailable` - 第三方服务不可达。
    /// * `AuthError::Internal` - 注册表未初始化或数据库写入失败。
    async fn link_oauth2_account(
        &self,
        user_id: &str,
        provider: &str,
        code: &str,
    ) -> std::result::Result<(), AuthError> {
        // 1. 获取 Provider
        let registry = crate::oauth2::OAuth2ProviderRegistry::get_global()
            .ok_or(AuthError::Internal("OAuth2 Provider 注册表未初始化".to_string()))?;
        let provider_impl = registry.get_provider(provider)?;

        // 2. 交换 Token
        let redirect_uri = provider_impl.redirect_uri().to_string();
        let token_response = provider_impl.exchange_code(code, &redirect_uri).await?;

        // 3. 获取用户信息
        let user_info = provider_impl.get_user_info(&token_response).await?;

        // 4. 检查该 Provider 账号是否已被其他用户绑定
        if self.account_linker.account_exists(provider, &user_info.provider_user_id).await? {
            return Err(AuthError::OAuth2(format!(
                "该 {} 账号已被其他用户绑定", provider
            )));
        }

        // 5. 创建关联记录
        self.account_linker.create_account(provider, &user_info.provider_user_id, user_id, &user_info).await?;

        // 审计日志：第三方账号绑定
        self.audit_log("oauth2_link", cmx_audit::OperationResult::Success, user_id, Some("user"), Some(user_id), Some(serde_json::json!({
            "provider": provider,
        }))).await;

        tracing::info!(user_id = %user_id, provider = %provider, "第三方账号绑定成功");
        Ok(())
    }

    /// 解绑用户的第三方 Provider 账号。
    ///
    /// 委托 `AccountLinker::unlink_account` 执行，包含安全检查：
    /// 若用户既无密码又无其他第三方绑定，则拒绝解绑最后一个登录方式。
    ///
    /// # Arguments
    ///
    /// * `user_id` - 本地用户 ID。
    /// * `provider` - 待解绑的第三方 Provider 名称。
    ///
    /// # Errors
    ///
    /// * `AuthError::OAuth2LastBindingCannotRemove` - 解绑后用户将无可用登录方式。
    /// * `AuthError::Internal` - 数据库操作失败。
    async fn unlink_oauth2_account(
        &self,
        user_id: &str,
        provider: &str,
    ) -> std::result::Result<(), AuthError> {
        self.account_linker.unlink_account(user_id, provider).await?;

        // 审计日志：第三方账号解绑
        self.audit_log("oauth2_unlink", cmx_audit::OperationResult::Success, user_id, Some("user"), Some(user_id), Some(serde_json::json!({
            "provider": provider,
        }))).await;

        Ok(())
    }

    /// 存储第三方 OAuth2 Provider 的 CSRF state。
    ///
    /// 在重定向用户到第三方授权页前调用，state 用于回调时校验请求来源，
    /// 防止 CSRF 攻击。
    ///
    /// # Arguments
    ///
    /// * `state` - 随机生成的 CSRF state 字符串。
    /// * `provider` - 关联的 Provider 名称，回调时校验一致性。
    ///
    /// # Errors
    ///
    /// 当 Redis 写入失败时返回 `AuthError::Internal`。
    async fn store_oauth2_provider_state(&self, state: &str, provider: &str) -> std::result::Result<(), AuthError> {
        self.oauth2_store.store_provider_state(state, provider).await
            .map_err(|e| AuthError::Internal(e.to_string()))
    }
}

#[async_trait]
impl AuthStorageQuery for AuthServiceImpl {
    /// 新增或更新 API Key 记录。
    ///
    /// 以 `key_prefix` 为唯一键执行 `INSERT ... ON CONFLICT DO UPDATE`，
    /// 已存在时覆盖 `key_hash`/`user_id`/`service_name`/`scopes`/`description`。
    ///
    /// # Arguments
    ///
    /// * `key_prefix` - API Key 前缀（唯一标识）。
    /// * `key_hash` - Key 的 SHA256 哈希。
    /// * `user_id` - 关联用户 ID（可选，纯服务间调用时为 `None`）。
    /// * `service_name` - 关联服务名称（可选）。
    /// * `scopes` - 允许的 scope 列表。
    /// * `description` - 描述/备注（可选）。
    ///
    /// # Errors
    ///
    /// 当 SQL 执行失败或 scopes 序列化失败时返回 `TraitError::Internal`。
    async fn upsert_api_key(
        &self,
        key_prefix: &str,
        key_hash: &str,
        user_id: Option<&str>,
        service_name: Option<&str>,
        scopes: &[String],
        description: Option<&str>,
    ) -> std::result::Result<(), cmx_traits::error::TraitError> {
        debug!(
            "{:<12} - AuthServiceImpl::upsert_api_key - key_prefix: {}",
            "AUTH", key_prefix
        );

        let id = snowflake_id_str();
        let user_id_val = user_id
            .map(|u| format!("'{}'", u.replace('\'', "''")))
            .unwrap_or("NULL".to_string());
        let service_name_val = service_name
            .map(|s| format!("'{}'", s.replace('\'', "''")))
            .unwrap_or("NULL".to_string());
        let scopes_json = serde_json::to_string(scopes)
            .map_err(|e| cmx_traits::error::TraitError::Internal(format!("序列化 scopes 失败: {}", e)))?;
        let description_val = description
            .map(|d| format!("'{}'", d.replace('\'', "''")))
            .unwrap_or("NULL".to_string());

        let sql = format!(
            "INSERT INTO cmx_auth_api_key (id, key_prefix, key_hash, user_id, service_name, scopes, description, archived, status) \
             VALUES ('{id}', '{key_prefix}', '{key_hash}', {user_id_val}, {service_name_val}, '{scopes_json}', {description_val}, 0, 1) \
             ON CONFLICT (key_prefix) DO UPDATE SET key_hash = EXCLUDED.key_hash, user_id = EXCLUDED.user_id, \
             service_name = EXCLUDED.service_name, scopes = EXCLUDED.scopes, description = EXCLUDED.description",
            id = id.replace('\'', "''"),
            key_prefix = key_prefix.replace('\'', "''"),
            key_hash = key_hash.replace('\'', "''"),
            user_id_val = user_id_val,
            service_name_val = service_name_val,
            scopes_json = scopes_json.replace('\'', "''"),
            description_val = description_val,
        );

        let db_manager = cmx_database::get_default_db_manager();
        let db_id = db_manager.get_default_db_id().await;
        db_manager
            .execute_sql(&db_id, None, &sql)
            .await
            .map_err(|e| cmx_traits::error::TraitError::Internal(format!("导入 API Key 失败: {}", e)))?;

        info!(key_prefix = key_prefix, "静态 API Key 已导入");
        Ok(())
    }

    /// 根据 `key_prefix` 查询未归档的 API Key 记录。
    ///
    /// # Arguments
    ///
    /// * `key_prefix` - API Key 前缀（前 8 位）。
    ///
    /// # Returns
    ///
    /// 存在时返回 `Some(ApiKeyData)`，否则返回 `None`。
    ///
    /// # Errors
    ///
    /// 当数据库查询失败时返回 `TraitError::Internal`。
    async fn get_api_key_by_prefix(
        &self,
        key_prefix: &str,
    ) -> std::result::Result<Option<cmx_traits::auth::ApiKeyData>, cmx_traits::error::TraitError> {
        debug!(
            "{:<12} - AuthServiceImpl::get_api_key_by_prefix - key_prefix: {}",
            "AUTH", key_prefix
        );

        let sql = format!(
            "SELECT key_prefix, key_hash, user_id, service_name, scopes, description, status \
             FROM cmx_auth_api_key WHERE key_prefix = '{}' AND archived = 0",
            key_prefix.replace('\'', "''")
        );

        let db_manager = cmx_database::get_default_db_manager();
        let db_id = db_manager.get_default_db_id().await;
        let dataset = db_manager
            .query_sql(&db_id, None, &sql, "api_key_by_prefix")
            .await
            .map_err(|e| cmx_traits::error::TraitError::Internal(format!("查询 API Key 失败: {}", e)))?;

        let schema = dataset.schema.as_ref();
        let row = match dataset.iter().next() {
            Some(r) => r,
            None => return Ok(None),
        };

        let scopes_str: String = row.get_by_name_as(schema, "scopes").unwrap_or_default();
        let scopes: Vec<String> = serde_json::from_str(&scopes_str).unwrap_or_default();

        Ok(Some(cmx_traits::auth::ApiKeyData {
            key_prefix: row.get_by_name_as(schema, "key_prefix").unwrap_or_default(),
            key_hash: row.get_by_name_as(schema, "key_hash").unwrap_or_default(),
            user_id: row.get_by_name_as(schema, "user_id"),
            service_name: row.get_by_name_as(schema, "service_name"),
            scopes,
            description: row.get_by_name_as(schema, "description"),
            status: row.get_by_name_as::<i64>(schema, "status").unwrap_or(1),
        }))
    }

    /// 记录 Token 生命周期事件到审计表。
    ///
    /// 用于 Token 签发、撤销、密码修改等关键事件的持久化审计。
    ///
    /// # Arguments
    ///
    /// * `event_type` - 事件类型（如 `token_issued`/`token_revoked`/`password_changed`）。
    /// * `user_id` - 关联用户 ID。
    /// * `jti` - 关联 Token 的 JTI（无关联时传空字符串）。
    /// * `detail` - 事件详情描述。
    ///
    /// # Errors
    ///
    /// 当 SQL 执行失败时返回 `TraitError::Internal`。
    async fn record_token_event(
        &self,
        event_type: &str,
        user_id: &str,
        jti: &str,
        detail: &str,
    ) -> std::result::Result<(), cmx_traits::error::TraitError> {
        debug!(
            "{:<12} - AuthServiceImpl::record_token_event - event: {}, user: {}",
            "AUTH", event_type, user_id
        );

        let id = snowflake_id_str();
        let sql = format!(
            "INSERT INTO cmx_auth_token_event (id, event_type, user_id, jti, detail, create_time) \
             VALUES ('{}', '{}', '{}', '{}', '{}', NOW())",
            id.replace('\'', "''"),
            event_type.replace('\'', "''"),
            user_id.replace('\'', "''"),
            jti.replace('\'', "''"),
            detail.replace('\'', "''"),
        );

        let db_manager = cmx_database::get_default_db_manager();
        let db_id = db_manager.get_default_db_id().await;
        db_manager
            .execute_sql(&db_id, None, &sql)
            .await
            .map_err(|e| cmx_traits::error::TraitError::Internal(format!("记录 Token 事件失败: {}", e)))?;

        Ok(())
    }

    /// 根据 `client_id` 查询未归档的 OAuth2 客户端信息。
    ///
    /// # Arguments
    ///
    /// * `client_id` - OAuth2 客户端 ID。
    ///
    /// # Returns
    ///
    /// 客户端存在时返回 `Some(OAuth2ClientData)`，否则返回 `None`。
    ///
    /// # Errors
    ///
    /// 当数据库查询失败时返回 `TraitError::Internal`。
    async fn get_oauth2_client(
        &self,
        client_id: &str,
    ) -> std::result::Result<Option<OAuth2ClientData>, cmx_traits::error::TraitError> {
        debug!(
            "{:<12} - AuthServiceImpl::get_oauth2_client - client_id: {}",
            "AUTH", client_id
        );

        let sql = format!(
            "SELECT client_id, client_name, client_secret, redirect_uris, grant_types, \
             client_type, pkce_required, allowed_scopes, status \
             FROM cmx_auth_client WHERE client_id = '{}' AND archived = 0",
            client_id.replace('\'', "''")
        );

        let db_manager = cmx_database::get_default_db_manager();
        let db_id = db_manager.get_default_db_id().await;
        let dataset = db_manager
            .query_sql(&db_id, None, &sql, "oauth2_client")
            .await
            .map_err(|e| cmx_traits::error::TraitError::Internal(format!("查询 OAuth2 客户端失败: {}", e)))?;

        let schema = dataset.schema.as_ref();
        let row = match dataset.iter().next() {
            Some(r) => r,
            None => return Ok(None),
        };

        // 解析 JSON 字段为 Vec<String>
        let redirect_uris_str: String =
            row.get_by_name_as(schema, "redirect_uris").unwrap_or_default();
        let redirect_uris: Vec<String> = serde_json::from_str(&redirect_uris_str).unwrap_or_default();

        let grant_types_str: String =
            row.get_by_name_as(schema, "grant_types").unwrap_or_default();
        let grant_types: Vec<String> = serde_json::from_str(&grant_types_str).unwrap_or_default();

        let allowed_scopes_str: String =
            row.get_by_name_as(schema, "allowed_scopes").unwrap_or_default();
        let allowed_scopes: Vec<String> = serde_json::from_str(&allowed_scopes_str).unwrap_or_default();

        let pkce_required: bool = row
            .get_by_name_as::<i64>(schema, "pkce_required")
            .map(|v| v != 0)
            .unwrap_or(true);

        Ok(Some(OAuth2ClientData {
            client_id: row.get_by_name_as(schema, "client_id").unwrap_or_default(),
            client_name: row.get_by_name_as(schema, "client_name").unwrap_or_default(),
            client_secret: row.get_by_name_as(schema, "client_secret"),
            redirect_uris,
            grant_types,
            client_type: row
                .get_by_name_as(schema, "client_type")
                .unwrap_or_else(|| "public".to_string()),
            pkce_required,
            allowed_scopes,
            status: row.get_by_name_as::<i64>(schema, "status").unwrap_or(1),
        }))
    }
}
