//! Auth Handler 实现
//!
//! 提供登录/登出/刷新Token/校验Token/修改密码等 HTTP API。

use axum::extract::State;
use axum::Json;
use cmx_traits::auth::{Credentials, DeviceInfo};
use tracing::{debug, info, warn};

use crate::{ApiResp, Error, Result};
use crate::app_state::CmxAppState;
use crate::middleware::CmxSvrContext;

use super::request::*;
use super::response::*;

/// 用户登录
#[utoipa::path(
    post,
    path = "/api/auth/login",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "登录成功或失败，body code 区分: 0=成功, 401=用户名或密码错误, 403=用户已被禁用", body = ApiResp<LoginResponse>),
    ),
    tag = "Auth"
)]
pub async fn auth_login(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Json(req): Json<LoginRequest>,
) -> Result<Json<ApiResp<LoginResponse>>> {
    debug!("{:<12} - handler::auth_login - username: {}", "HANDLER", req.username);

    let auth_service = cmx_state.auth_service().ok_or_else(|| {
        Error::InternalError("认证服务未初始化".to_string())
    })?;

    let credentials = Credentials::Password {
        username: req.username.clone(),
        password: req.password.clone(),
    };

    let device_info = DeviceInfo {
        device_type: req.device_type.clone(),
        device_id: req.device_id.clone(),
        ip: None,
        user_agent: None,
    };

    let token_pair = match auth_service
        .authenticate(credentials, Some(device_info))
        .await
    {
        Ok(pair) => pair,
        Err(cmx_traits::auth::AuthError::InvalidCredentials) => {
            warn!(username = %req.username, "用户名或密码错误");
            return Ok(Json(ApiResp::fail(401, "未授权: 用户名或密码错误")));
        }
        Err(cmx_traits::auth::AuthError::UserDisabled) => {
            warn!(username = %req.username, "用户已被禁用");
            return Ok(Json(ApiResp::fail(403, "用户已被禁用")));
        }
        Err(cmx_traits::auth::AuthError::TooManyAttempts { secs, limit, window }) => {
            // 5.1 修复：映射为 429 Too Many Requests 而非 403
            return Err(Error::RateLimitExceeded {
                retry_after: secs,
                limit: limit as u64,
                window,
            });
        }
        Err(other) => return Err(Error::InternalError(other.to_string())),
    };

    let response = LoginResponse {
        access_token: token_pair.access_token,
        refresh_token: token_pair.refresh_token,
        token_type: token_pair.token_type,
        access_expires_at: token_pair.access_expires_at,
        refresh_expires_at: token_pair.refresh_expires_at,
    };

    info!(username = %req.username, "用户登录成功");
    Ok(Json(ApiResp::ok(response)))
}

/// 刷新 Token
#[utoipa::path(
    post,
    path = "/api/auth/refresh",
    request_body = RefreshRequest,
    responses(
        (status = 200, description = "刷新成功", body = LoginResponse),
        (status = 401, description = "Refresh Token 无效")
    ),
    tag = "Auth"
)]
pub async fn auth_refresh(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Json(req): Json<RefreshRequest>,
) -> Result<Json<ApiResp<LoginResponse>>> {
    debug!("{:<12} - handler::auth_refresh", "HANDLER");

    let auth_service = cmx_state.auth_service().ok_or_else(|| {
        Error::InternalError("认证服务未初始化".to_string())
    })?;

    let token_pair = auth_service
        .refresh_token(&req.refresh_token)
        .await
        .map_err(|e| match e {
            cmx_traits::auth::AuthError::TokenExpired
            | cmx_traits::auth::AuthError::TokenRevoked
            | cmx_traits::auth::AuthError::ReplayDetected
            | cmx_traits::auth::AuthError::InvalidToken(_) => {
                Error::Unauthorized(e.to_string())
            }
            other => Error::InternalError(other.to_string()),
        })?;

    let response = LoginResponse {
        access_token: token_pair.access_token,
        refresh_token: token_pair.refresh_token,
        token_type: token_pair.token_type,
        access_expires_at: token_pair.access_expires_at,
        refresh_expires_at: token_pair.refresh_expires_at,
    };

    Ok(Json(ApiResp::ok(response)))
}

/// 登出（撤销当前 Token）
#[utoipa::path(
    post,
    path = "/api/auth/logout",
    request_body = RevokeRequest,
    responses(
        (status = 200, description = "登出成功")
    ),
    tag = "Auth"
)]
pub async fn auth_logout(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(svr_ctx): CmxSvrContext,
    Json(req): Json<RevokeRequest>,
) -> Result<Json<ApiResp<()>>> {
    debug!("{:<12} - handler::auth_logout", "HANDLER");

    let auth_service = cmx_state.auth_service().ok_or_else(|| {
        Error::InternalError("认证服务未初始化".to_string())
    })?;

    auth_service
        .revoke_token(&req.token)
        .await
        .map_err(|e| Error::InternalError(e.to_string()))?;

    if let Some(auth_ctx) = &svr_ctx.auth_context {
        info!(user_id = %auth_ctx.user_id, "用户登出");
    }

    Ok(Json(ApiResp::ok(())))
}

/// 校验 Token
#[utoipa::path(
    post,
    path = "/api/auth/validate",
    request_body = ValidateRequest,
    responses(
        (status = 200, description = "Token 有效", body = ValidateResponse),
        (status = 401, description = "Token 无效")
    ),
    tag = "Auth"
)]
pub async fn auth_validate(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Json(req): Json<ValidateRequest>,
) -> Result<Json<ApiResp<ValidateResponse>>> {
    debug!("{:<12} - handler::auth_validate", "HANDLER");

    let auth_service = cmx_state.auth_service().ok_or_else(|| {
        Error::InternalError("认证服务未初始化".to_string())
    })?;

    let auth_ctx = auth_service
        .validate_token(&req.token)
        .await
        .map_err(|e| match e {
            cmx_traits::auth::AuthError::TokenExpired
            | cmx_traits::auth::AuthError::TokenRevoked
            | cmx_traits::auth::AuthError::InvalidToken(_) => {
                Error::Unauthorized(e.to_string())
            }
            other => Error::InternalError(other.to_string()),
        })?;

    let response = ValidateResponse {
        user_id: auth_ctx.user_id,
        username: auth_ctx.username,
        roles: auth_ctx.roles,
        permissions: auth_ctx.permissions,
        session_id: auth_ctx.session_id,
        device_type: auth_ctx.device_type,
        auth_method: auth_ctx.auth_method,
    };

    Ok(Json(ApiResp::ok(response)))
}

/// 撤销用户所有 Token（管理员强制下线）
///
/// P1-6.4: 需要调用者具有 `system:auth:kick` 权限
#[utoipa::path(
    post,
    path = "/api/auth/revoke-all",
    request_body = RevokeAllRequest,
    responses(
        (status = 200, description = "操作成功"),
        (status = 403, description = "无权限")
    ),
    tag = "Auth"
)]
pub async fn auth_revoke_all(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(svr_ctx): CmxSvrContext,
    Json(req): Json<RevokeAllRequest>,
) -> Result<Json<ApiResp<()>>> {
    debug!("{:<12} - handler::auth_revoke_all - user_id: {}", "HANDLER", req.user_id);

    // P1-6.4: 权限校验 — 需要 system:auth:kick 权限
    if let Some(auth_ctx) = &svr_ctx.auth_context {
        if !auth_ctx.permissions.contains(&"system:auth:kick".to_string()) {
            return Err(Error::Forbidden("无权执行强制下线操作，需要 system:auth:kick 权限".to_string()));
        }
    } else {
        return Err(Error::Forbidden("未认证".to_string()));
    }

    let auth_service = cmx_state.auth_service().ok_or_else(|| {
        Error::InternalError("认证服务未初始化".to_string())
    })?;

    auth_service
        .revoke_all_tokens(&req.user_id)
        .await
        .map_err(|e| Error::InternalError(e.to_string()))?;

    warn!(user_id = %req.user_id, "管理员强制下线用户");
    Ok(Json(ApiResp::ok(())))
}

/// 会话心跳（刷新会话活跃时间）
#[utoipa::path(
    post,
    path = "/api/auth/heartbeat",
    responses(
        (status = 200, description = "心跳成功"),
        (status = 401, description = "会话不存在或已过期")
    ),
    tag = "Auth"
)]
pub async fn auth_heartbeat(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(svr_ctx): CmxSvrContext,
) -> Result<Json<ApiResp<()>>> {
    let auth_ctx = svr_ctx.auth_context.ok_or_else(|| {
        Error::Unauthorized("未认证".to_string())
    })?;

    let auth_service = cmx_state.auth_service().ok_or_else(|| {
        Error::InternalError("认证服务未初始化".to_string())
    })?;

    let device_type = auth_ctx.device_type.as_deref().unwrap_or("unknown");

    let refreshed = auth_service
        .heartbeat(&auth_ctx.user_id, device_type)
        .await
        .map_err(|e| Error::InternalError(e.to_string()))?;

    if !refreshed {
        return Err(Error::Unauthorized("会话不存在或已过期".to_string()));
    }

    Ok(Json(ApiResp::ok(())))
}

/// 认证服务健康检查
#[utoipa::path(
    get,
    path = "/api/auth/health",
    responses(
        (status = 200, description = "服务健康")
    ),
    tag = "Auth"
)]
pub async fn auth_health() -> Result<Json<ApiResp<serde_json::Value>>> {
    let mut checks = serde_json::Map::new();
    let mut all_healthy = true;

    // 检查 Redis 连通性
    let redis_healthy = match cmx_buffer::GlobalCacheManager::try_get() {
        Some(cm) => cm.ops().exists("auth:health:check").await.is_ok(),
        None => false,
    };
    checks.insert("redis".to_string(), serde_json::Value::Bool(redis_healthy));
    if !redis_healthy {
        all_healthy = false;
    }

    // 检查 JWT 密钥可用性（如果认证服务已初始化则密钥可用）
    let auth_healthy = crate::middleware::GlobalAuthService::get().is_some();
    checks.insert(
        "jwt_keys".to_string(),
        serde_json::Value::Bool(auth_healthy),
    );
    if !auth_healthy {
        all_healthy = false;
    }

    checks.insert(
        "status".to_string(),
        if all_healthy {
            "healthy".into()
        } else {
            "degraded".into()
        },
    );

    Ok(Json(ApiResp::ok(serde_json::Value::Object(checks))))
}

/// 修改密码
#[utoipa::path(
    post,
    path = "/api/auth/change-password",
    request_body = ChangePasswordRequest,
    responses(
        (status = 200, description = "修改成功"),
        (status = 400, description = "密码不符合策略或与历史重复"),
        (status = 401, description = "旧密码错误")
    ),
    tag = "Auth"
)]
pub async fn auth_change_password(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(svr_ctx): CmxSvrContext,
    Json(req): Json<ChangePasswordRequest>,
) -> Result<Json<ApiResp<()>>> {
    let auth_ctx = svr_ctx.auth_context.ok_or_else(|| {
        Error::Unauthorized("未认证".to_string())
    })?;

    let auth_service = cmx_state.auth_service().ok_or_else(|| {
        Error::InternalError("认证服务未初始化".to_string())
    })?;

    auth_service
        .change_password(&auth_ctx.user_id, &req.old_password, &req.new_password)
        .await
        .map_err(|e| match e {
            cmx_traits::auth::AuthError::InvalidCredentials => {
                Error::Unauthorized("旧密码错误".to_string())
            }
            cmx_traits::auth::AuthError::PasswordPolicyViolated(msg) => {
                Error::BadRequest(msg)
            }
            cmx_traits::auth::AuthError::PasswordReused => {
                Error::BadRequest("新密码与历史密码重复".to_string())
            }
            other => Error::InternalError(other.to_string()),
        })?;

    info!(user_id = %auth_ctx.user_id, "用户修改密码成功");
    Ok(Json(ApiResp::ok(())))
}
