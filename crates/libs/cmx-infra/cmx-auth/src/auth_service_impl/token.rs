//! Token 生命周期
//!
//! 实现 Token 签发、校验、刷新、撤销（单个/全部）、会话心跳、本地缓存失效，
//! 以及 Pub/Sub 缓存失效广播与 Token 审计事件记录。

use std::time::Duration;

use chrono::Utc;
use cmx_core::AuthContext;
use cmx_traits::auth::{AuthError, AuthStorageQuery, DeviceInfo, TokenPair};
use tracing::{debug, info, warn};

use crate::auth_service_impl::{AuthServiceImpl, CACHE_INVALIDATE_CHANNEL};
use crate::metrics;
use crate::session::UserSession;

impl AuthServiceImpl {
    /// 签发 Token 对
    pub(super) async fn issue_token_pair(
        &self,
        user_id: &str,
        username: &str,
        nickname: Option<&str>,
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
                nickname,
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
            device_id: device_info.map(|d| d.device_id.clone()).unwrap_or_default(),
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
        self.audit_token_event(
            "token_issued",
            user_id,
            &session.access_jti,
            "access_token_issued",
        )
        .await;

        let now_ts = Utc::now().timestamp();
        Ok(TokenPair {
            access_token,
            refresh_token,
            access_expires_at: now_ts + self.config.token.access_ttl_secs as i64,
            refresh_expires_at: now_ts + self.config.token.refresh_ttl_secs as i64,
            token_type: "Bearer".to_string(),
        })
    }

    /// P0-2.3: 发布 Pub/Sub 缓存失效消息
    pub(super) async fn publish_cache_invalidate(&self, message: &str) {
        if let Err(e) = self
            .cache
            .pubsub()
            .publish(CACHE_INVALIDATE_CHANNEL, message)
            .await
        {
            warn!(channel = CACHE_INVALIDATE_CHANNEL, error = %e, "Pub/Sub 缓存失效消息发布失败");
        }
    }

    /// 记录 Token 审计日志
    pub(super) async fn audit_token_event(
        &self,
        event_type: &str,
        user_id: &str,
        jti: &str,
        detail: &str,
    ) {
        if let Err(e) =
            AuthStorageQuery::record_token_event(self, event_type, user_id, jti, detail).await
        {
            warn!(event_type = event_type, user_id = user_id, error = %e, "审计日志写入失败");
        }
    }

    /// 验证 Access Token 并返回 `AuthContext`。
    ///
    /// 解码 Token → 检查黑名单 → 检查会话活跃度，并记录验证耗时指标。
    pub(super) async fn validate_token(
        &self,
        token: &str,
    ) -> std::result::Result<AuthContext, AuthError> {
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
    pub(super) async fn refresh_token(
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
            user.nickname.as_deref(),
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
    pub(super) async fn revoke_token(&self, token: &str) -> std::result::Result<(), AuthError> {
        // 尝试解码为 Access Token
        if let Ok(claims) = self.jwt_manager.decode_access_token(token) {
            let remaining =
                Duration::from_secs((claims.exp - Utc::now().timestamp()).max(0) as u64);
            self.token_manager
                .blacklist_access_token(&claims.jti, remaining)
                .await
                .map_err(|e| AuthError::Internal(e.to_string()))?;
            // P0-2.3: Pub/Sub 广播本地缓存失效
            self.publish_cache_invalidate(&format!("blacklist:{}", claims.jti))
                .await;
            info!(jti = %claims.jti, "Access Token 已撤销");
            metrics::record_token_revoked("access");
            self.audit_token_event(
                "token_revoked",
                &claims.sub,
                &claims.jti,
                "single_token_revoked",
            )
            .await;
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
            self.audit_token_event(
                "token_revoked",
                &claims.sub,
                &claims.jti,
                "single_token_revoked",
            )
            .await;
            return Ok(());
        }

        Err(AuthError::InvalidToken("无法解码 Token".to_string()))
    }

    /// 撤销用户的所有 Token 与会话。
    ///
    /// 撤销所有 Refresh Token + 将所有 Access Token 加入黑名单（TTL 取剩余有效期）+
    /// 销毁所有会话 + Pub/Sub 广播本地缓存失效。
    pub(super) async fn revoke_all_tokens(
        &self,
        user_id: &str,
    ) -> std::result::Result<(), AuthError> {
        // 撤销所有 Refresh Token
        // 此步失败属于关键错误，直接返回（Refresh Token 未撤销会导致用户仍可刷新 Token）
        self.token_manager
            .revoke_all_refresh_tokens(user_id)
            .await
            .map_err(|e| AuthError::Internal(e.to_string()))?;

        // P1-3.6: 获取每个会话的 access_jti，将 Access Token 加入黑名单
        // get_user_devices 失败属于关键错误，无法继续后续销毁流程
        let devices = self
            .session_manager
            .get_user_devices(user_id)
            .await
            .map_err(|e| AuthError::Internal(e.to_string()))?;

        // 收集销毁失败的设备，用于审计日志
        let mut failed_devices: Vec<String> = Vec::new();

        for device in &devices {
            // 获取会话信息，根据结果决定后续处理
            let session_result = self.session_manager.get_session(user_id, device).await;
            match session_result {
                Ok(Some(session)) => {
                    // 2.2 修复：使用 Access Token 实际剩余有效期，而非完整 access_ttl_secs
                    let remaining_secs =
                        (session.access_expires_at - Utc::now().timestamp()).max(0) as u64;
                    let remaining = Duration::from_secs(remaining_secs);
                    // blacklist 失败不影响后续销毁，Access Token 会在过期后自然失效
                    if let Err(e) = self
                        .token_manager
                        .blacklist_access_token(&session.access_jti, remaining)
                        .await
                    {
                        warn!(user_id = user_id, device = %device, error = %e, "Access Token 加入黑名单失败，将依赖自然过期");
                    }
                }
                Ok(None) => {
                    // 会话已不存在，无需黑名单处理也无需销毁，跳过该设备
                    debug!(user_id = user_id, device = %device, "会话不存在，跳过黑名单与销毁");
                    continue;
                }
                Err(e) => {
                    // 获取会话失败（如 Redis 临时故障），跳过黑名单但仍尝试销毁
                    // destroy_session 内部会处理不存在的情况，此处尝试销毁可清理残留的设备注册信息
                    warn!(user_id = user_id, device = %device, error = %e, "获取会话信息失败，跳过黑名单处理");
                }
            }

            // 销毁会话失败时记录 warn 并继续处理其他设备，避免单个设备失败导致整个撤销流程中断
            if let Err(e) = self.session_manager.destroy_session(user_id, device).await {
                warn!(user_id = user_id, device = %device, error = %e, "销毁会话失败，继续处理其他设备");
                failed_devices.push(device.clone());
            }
        }

        // P0-2.3: Pub/Sub 广播全部本地缓存失效
        // 无论个别设备销毁是否失败，都执行广播和本地缓存失效：
        // - Refresh Token 已在开头撤销，即使会话残留也无法刷新 Token
        // - 广播确保其他实例尽快清空本地缓存，避免使用过期的权限快照
        // - 残留的 Access Token 会按自身 TTL 自然过期，安全风险可控
        self.publish_cache_invalidate(&format!("revoke_all:{}", user_id))
            .await;
        // 本实例也立即失效本地缓存
        self.token_manager.invalidate_local_cache_all().await;

        if failed_devices.is_empty() {
            info!(user_id = user_id, "用户所有 Token 已撤销");
        } else {
            // 部分设备销毁失败：缓存已失效，但 Redis 中可能残留部分会话记录
            // 残留会话的 Access Token 会按 TTL 自然过期，无需人工干预
            warn!(
                user_id = user_id,
                failed_count = failed_devices.len(),
                total_devices = devices.len(),
                failed_devices = ?failed_devices,
                "用户 Token 撤销完成，但部分设备会话销毁失败（缓存已失效，残留 Access Token 将按 TTL 自然过期）"
            );
        }
        self.audit_token_event("tokens_revoked", user_id, "", "all_tokens_revoked")
            .await;
        Ok(())
    }

    /// 刷新指定用户/设备的会话心跳。
    ///
    /// 更新会话的 `last_active_at` 字段并刷新 `session_detail` 的 TTL，
    /// 用于维持会话活跃状态。
    pub(super) async fn heartbeat(
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
    pub(super) async fn invalidate_local_cache(&self, message: &str) {
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
}
