//! API Key 验证、超管初始化与静态 Key 导入
//!
//! 实现 [`cmx_traits::auth::AuthService`] 的 API Key 验证（含两层缓存）、
//! 超管账号初始化、静态 API Key 导入，以及内部 API Key 实体验证辅助方法。
//! 同时包含后台会话清理任务（生命周期管理，非 trait 方法）。

use std::time::Duration;

use chrono::Utc;
use cmx_core::AuthContext;
use cmx_traits::auth::{AuthError, AuthStorageQuery};
use tracing::{info, warn};

use crate::api_key::ApiKeyEntity;
use crate::auth_service_impl::{AuthServiceImpl, API_KEY_CTX_CACHE_TTL_SECS};
use crate::metrics;
use crate::session::UserSession;

impl AuthServiceImpl {
    /// 验证 API Key 并返回实体（直接调用 AuthStorageQuery，无需外部 trait 对象）
    pub(super) async fn validate_api_key_entity(
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
    pub(super) async fn validate_api_key(&self, key: &str) -> std::result::Result<AuthContext, AuthError> {
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
                tracing::debug!(key_prefix = %key_prefix, "API Key AuthContext 缓存命中，跳过 user/roles/permissions 查询");
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
            tracing::debug!(key_prefix = %key_prefix, "API Key AuthContext 已写入缓存");
        }

        Ok(auth_ctx)
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
    pub(super) async fn ensure_super_admin(&self) -> std::result::Result<(), AuthError> {
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
    pub(super) async fn import_static_api_keys(&self) -> std::result::Result<(), AuthError> {
        if self.config.static_api_keys.is_empty() {
            info!("没有静态 API Key 配置");
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
    ///
    /// 返回 `JoinHandle` 供调用方管理任务生命周期（如 shutdown 时 `abort()` 或等待完成）。
    /// 此方法为同步函数（非 `async fn`），因为内部仅做 `tokio::spawn` 后立即返回。
    ///
    /// 注意：此方法不属于 `AuthService` trait，因为它是生命周期管理方法，
    /// 仅在应用启动时调用一次，不应暴露给业务调用方。
    pub fn start_cleanup_task(&self) -> tokio::task::JoinHandle<()> {
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
        })
    }
}
