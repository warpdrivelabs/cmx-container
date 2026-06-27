//! AuthService trait 实现
//!
//! [`cmx_traits::auth::AuthService`] 与 [`cmx_traits::auth::AuthStorageQuery`] 的默认实现
//! `AuthServiceImpl`，整合 `JwtManager`、`Argon2Hasher`、`TokenManager`、`SessionManager`、
//! `OAuth2Policy` 等子模块。
//!
//! 本模块按职责拆分为多个子模块，结构体定义、构造器与两个 trait 的薄委托实现保留在此处，
//! 各功能方法的 `impl` 块分散到子模块：
//!
//! - [`login`]：登录认证（密码认证、凭据分派、凭据校验、登录失败计数）
//! - [`token`]：Token 签发/校验/刷新/撤销、会话心跳、本地缓存失效、Pub/Sub 广播、Token 审计事件
//! - [`password`]：密码哈希、校验、修改（含策略+历史校验链）
//! - [`apikey`]：API Key 验证（两层缓存）、超管初始化、静态 Key 导入、后台会话清理任务
//! - [`oauth2`]：第三方 OAuth2 全流程（登录、回调、exchange、绑定/解绑、state、列表）
//! - [`user_info`]：用户信息聚合查询、审计日志辅助
//! - [`storage_query`]：`AuthStorageQuery` 的 API Key/Token 事件/OAuth2 客户端持久化
//!
//! Rust 要求一个类型对同一 trait 只能有一个 `impl`，因此本文件集中委派两个 trait，
//! 实现逻辑分散在各子模块的 `impl AuthServiceImpl` 固有方法块中。
//! trait 委托调用同名固有方法时，固有方法优先于 trait 方法解析，故不会递归。

use std::sync::Arc;

use async_trait::async_trait;
use cmx_buffer::CacheManager;
use cmx_core::AuthContext;
use cmx_traits::auth::{
    AuthError, AuthService, AuthStorageQuery, Credentials, DeviceInfo, OAuth2CallbackExchangeResult,
    OAuth2CallbackResult, OAuth2ClientData, TokenPair, UserInfo, UserAuthQuery,
};
use crate::config::AuthConfig;
use crate::error::Result;
use crate::jwt::JwtManager;
use crate::oauth2::provider::AccountLinker;
use crate::password::{Argon2Hasher, PasswordHistory, PasswordPolicy};
use crate::policy::OAuth2Policy;
use crate::session::SessionManager;
use crate::token::TokenManager;

mod apikey;
mod login;
mod oauth2;
mod password;
mod storage_query;
mod token;
mod user_info;

/// Pub/Sub 频道名。
const CACHE_INVALIDATE_CHANNEL: &str = "auth:cache:invalidate";

/// API Key AuthContext 缓存 TTL（秒）。
const API_KEY_CTX_CACHE_TTL_SECS: u64 = 60;

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
    pub fn with_audit_logger(mut self, logger: Arc<dyn cmx_audit::AuditLogger>) -> Self {
        self.audit_logger = Some(logger);
        self
    }

    /// 获取 OAuth2 策略引用。
    pub fn oauth2_policy(&self) -> &OAuth2Policy {
        &self.oauth2_policy
    }
}

/// `AuthService` 的唯一实现。
///
/// 各方法体委托给按职责拆分到子模块（[`login`] / [`token`] / [`password`] /
/// [`apikey`] / [`oauth2`] / [`user_info`]）中的固有方法。
/// 委托调用同名固有方法时，Rust 的固有方法优先级保证解析到子模块实现，
/// 不会回调本 trait 方法（无递归）。
#[async_trait]
impl AuthService for AuthServiceImpl {
    async fn authenticate(
        &self,
        credentials: Credentials,
        device_info: Option<DeviceInfo>,
    ) -> std::result::Result<TokenPair, AuthError> {
        self.authenticate(credentials, device_info).await
    }

    async fn validate_token(&self, token: &str) -> std::result::Result<AuthContext, AuthError> {
        self.validate_token(token).await
    }

    async fn refresh_token(&self, refresh_token: &str) -> std::result::Result<TokenPair, AuthError> {
        self.refresh_token(refresh_token).await
    }

    async fn revoke_token(&self, token: &str) -> std::result::Result<(), AuthError> {
        self.revoke_token(token).await
    }

    async fn revoke_all_tokens(&self, user_id: &str) -> std::result::Result<(), AuthError> {
        self.revoke_all_tokens(user_id).await
    }

    async fn hash_password(&self, plain: &str) -> std::result::Result<String, AuthError> {
        self.hash_password(plain).await
    }

    async fn verify_password(
        &self,
        plain: &str,
        hash: &str,
    ) -> std::result::Result<bool, AuthError> {
        self.verify_password(plain, hash).await
    }

    async fn heartbeat(
        &self,
        user_id: &str,
        device_type: &str,
    ) -> std::result::Result<bool, AuthError> {
        self.heartbeat(user_id, device_type).await
    }

    async fn invalidate_local_cache(&self, message: &str) {
        self.invalidate_local_cache(message).await
    }

    async fn change_password(
        &self,
        user_id: &str,
        old_password: &str,
        new_password: &str,
    ) -> std::result::Result<(), AuthError> {
        self.change_password(user_id, old_password, new_password)
            .await
    }

    async fn ensure_super_admin(&self) -> std::result::Result<(), AuthError> {
        self.ensure_super_admin().await
    }

    async fn import_static_api_keys(&self) -> std::result::Result<(), AuthError> {
        self.import_static_api_keys().await
    }

    async fn get_oauth2_client(
        &self,
        client_id: &str,
    ) -> std::result::Result<Option<OAuth2ClientData>, AuthError> {
        // 委托给 storage_query 子模块的固有方法（返回 TraitError），映射为 AuthError
        self.get_oauth2_client(client_id)
            .await
            .map_err(|e| AuthError::Internal(e.to_string()))
    }

    async fn verify_credentials(
        &self,
        username: &str,
        password: &str,
    ) -> std::result::Result<String, AuthError> {
        self.verify_credentials(username, password).await
    }

    async fn validate_api_key(&self, key: &str) -> std::result::Result<AuthContext, AuthError> {
        self.validate_api_key(key).await
    }

    async fn get_user_info(&self, user_id: &str) -> std::result::Result<UserInfo, AuthError> {
        self.get_user_info(user_id).await
    }

    async fn list_oauth2_providers(
        &self,
    ) -> std::result::Result<Vec<cmx_traits::auth::ProviderInfo>, AuthError> {
        self.list_oauth2_providers().await
    }

    async fn handle_oauth2_callback(
        &self,
        provider: &str,
        code: &str,
        state: &str,
        device_info: Option<DeviceInfo>,
    ) -> std::result::Result<OAuth2CallbackResult, AuthError> {
        self.handle_oauth2_callback(provider, code, state, device_info)
            .await
    }

    async fn exchange_oauth2_callback_code(
        &self,
        code: &str,
        state: &str,
        device_info: Option<DeviceInfo>,
    ) -> std::result::Result<OAuth2CallbackExchangeResult, AuthError> {
        self.exchange_oauth2_callback_code(code, state, device_info)
            .await
    }

    async fn link_oauth2_account(
        &self,
        user_id: &str,
        provider: &str,
        code: &str,
    ) -> std::result::Result<(), AuthError> {
        self.link_oauth2_account(user_id, provider, code).await
    }

    async fn unlink_oauth2_account(
        &self,
        user_id: &str,
        provider: &str,
    ) -> std::result::Result<(), AuthError> {
        self.unlink_oauth2_account(user_id, provider).await
    }

    async fn store_oauth2_provider_state(
        &self,
        state: &str,
        provider: &str,
    ) -> std::result::Result<(), AuthError> {
        self.store_oauth2_provider_state(state, provider).await
    }
}

/// `AuthStorageQuery` 的唯一实现。
///
/// 各方法体委托给 [`storage_query`] 子模块中的固有方法。
#[async_trait]
impl AuthStorageQuery for AuthServiceImpl {
    async fn upsert_api_key(
        &self,
        key_prefix: &str,
        key_hash: &str,
        user_id: Option<&str>,
        service_name: Option<&str>,
        scopes: &[String],
        description: Option<&str>,
    ) -> std::result::Result<(), cmx_traits::error::TraitError> {
        self.upsert_api_key(key_prefix, key_hash, user_id, service_name, scopes, description)
            .await
    }

    async fn get_api_key_by_prefix(
        &self,
        key_prefix: &str,
    ) -> std::result::Result<Option<cmx_traits::auth::ApiKeyData>, cmx_traits::error::TraitError> {
        self.get_api_key_by_prefix(key_prefix).await
    }

    async fn record_token_event(
        &self,
        event_type: &str,
        user_id: &str,
        jti: &str,
        detail: &str,
    ) -> std::result::Result<(), cmx_traits::error::TraitError> {
        self.record_token_event(event_type, user_id, jti, detail)
            .await
    }

    async fn get_oauth2_client(
        &self,
        client_id: &str,
    ) -> std::result::Result<Option<OAuth2ClientData>, cmx_traits::error::TraitError> {
        self.get_oauth2_client(client_id).await
    }
}
