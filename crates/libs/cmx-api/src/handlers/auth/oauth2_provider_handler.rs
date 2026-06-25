//! 第三方 OAuth2 Provider Handler
//!
//! 提供第三方 OAuth2 登录的 HTTP API，包括：
//! - 列出 Provider
//! - 授权重定向
//! - 回调处理
//! - 授权码换 Token
//! - 绑定/解绑

use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::Json;
use serde::Deserialize;
use tracing::{error, info};

use crate::app_state::CmxAppState;
use crate::middleware::{CmxSvrContext, GlobalAuthService};
use crate::{ApiResp, Error, Result};

/// 列出所有已启用的第三方 OAuth2 Provider
pub async fn oauth2_providers(
    State(state): State<CmxAppState>,
) -> Result<Json<ApiResp<Vec<cmx_traits::auth::ProviderInfo>>>> {
    let auth_service = state.auth_service().ok_or_else(|| {
        Error::InternalError("认证服务未初始化".to_string())
    })?;

    let providers = auth_service.list_oauth2_providers().await.map_err(|e| {
        error!(error = %e, "列出 OAuth2 Provider 失败");
        Error::InternalError(e.to_string())
    })?;

    Ok(Json(ApiResp::ok(providers)))
}

/// 获取 Provider 授权 URL 并重定向
pub async fn oauth2_provider_authorize(
    State(state): State<CmxAppState>,
    Path(provider): Path<String>,
) -> Result<axum::response::Redirect> {
    // 1. 获取 Provider Registry
    let registry = GlobalAuthService::get_provider_registry().ok_or_else(|| {
        Error::InternalError("OAuth2 Provider 注册表未初始化".to_string())
    })?;

    // 2. 获取 Provider
    let provider_impl = registry.get_provider(&provider).map_err(|e| {
        Error::BadRequest(e.to_string())
    })?;

    // 3. 生成 state 并通过 AuthService 存储
    let state_value = uuid::Uuid::new_v4().to_string();
    let redirect_uri = provider_impl.redirect_uri().to_string();

    let auth_service = state.auth_service().ok_or_else(|| {
        Error::InternalError("认证服务未初始化".to_string())
    })?;
    auth_service.store_oauth2_provider_state(&state_value, &provider).await.map_err(|e| {
        error!(error = %e, "存储 OAuth2 state 失败");
        Error::InternalError("存储 state 失败".to_string())
    })?;

    // 4. 构建授权 URL
    let scopes = provider_impl.default_scopes();
    let authorize_url = provider_impl.build_authorize_url(&state_value, &redirect_uri, &scopes);

    info!(provider = %provider, state = %state_value, "重定向到 Provider 授权页面");
    Ok(axum::response::Redirect::temporary(&authorize_url))
}

/// Provider 回调请求查询参数。
///
/// 由第三方 Provider 在用户授权后重定向到本端时携带，含授权码和 CSRF state。
#[derive(Debug, Deserialize)]
pub struct CallbackParams {
    /// 授权码。
    pub code: String,
    /// CSRF state，由 authorize 阶段生成并由 Provider 原样回传。
    pub state: String,
}

/// Provider 回调（交换 Token + 获取用户信息 + 关联/注册 + 签发一次性授权码 + 重定向前端）
pub async fn oauth2_provider_callback(
    State(state): State<CmxAppState>,
    Path(provider): Path<String>,
    Query(params): Query<CallbackParams>,
    headers: HeaderMap,
) -> Result<axum::response::Redirect> {
    let auth_service = state.auth_service().ok_or_else(|| {
        Error::InternalError("认证服务未初始化".to_string())
    })?;

    let device_info = extract_device_info(&headers);

    match auth_service.handle_oauth2_callback(&provider, &params.code, &params.state, device_info).await {
        Ok(result) => {
            let frontend_url = get_frontend_callback_url()?;
            let redirect_url = format!(
                "{}?code={}&state={}",
                frontend_url,
                result.callback_code,
                result.state
            );
            info!(provider = %provider, is_new = result.is_new, "第三方 OAuth2 回调成功，重定向前端");
            Ok(axum::response::Redirect::temporary(&redirect_url))
        }
        Err(e) => {
            error!(provider = %provider, error = %e, "第三方 OAuth2 回调失败");
            let frontend_url = get_frontend_callback_url()?;
            let error_code = sanitize_oauth2_error(&e);
            let redirect_url = format!("{}?error={}&state={}", frontend_url, error_code, params.state);
            Ok(axum::response::Redirect::temporary(&redirect_url))
        }
    }
}

/// 一次性授权码换 Token 请求载荷。
///
/// 第三方 OAuth2 登录回调后，前端收到一次性授权码后调用此接口换发 Access/Refresh Token。
/// 一次性码仅可使用一次且有效期短（建议 60 秒）。
#[derive(Debug, Deserialize)]
pub struct ExchangeCodeRequest {
    /// 一次性授权码。
    pub code: String,
    /// 原始 state（用于前端校验 CSRF）。
    pub state: String,
}

/// 一次性授权码换 Token 响应载荷。
///
/// `is_new` 标识是否为首次登录的新注册用户，前端可据此提示用户完善资料。
#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct ExchangeCodeResponse {
    /// Access Token。
    pub access_token: String,
    /// Refresh Token。
    pub refresh_token: String,
    /// Token 类型，固定为 "Bearer"。
    pub token_type: String,
    /// Access Token 过期时间（Unix 时间戳）。
    pub access_expires_at: i64,
    /// Refresh Token 过期时间（Unix 时间戳）。
    pub refresh_expires_at: i64,
    /// 是否为新注册用户。
    pub is_new: bool,
    /// Provider 名称（如 google/github）。
    pub provider: String,
    /// 原始 state（用于前端校验 CSRF）。
    pub state: String,
}

/// 用授权码换 TokenPair（统一接口，后端自动判断模式）。
///
/// 后端自动判断两种模式：
/// - 后端回调模式：`code` 为 `handle_oauth2_callback` 签发的一次性回调码
/// - 前端直调模式：`code` 为 Provider 返回的原始授权码
pub async fn oauth2_provider_exchange(
    State(state): State<CmxAppState>,
    headers: HeaderMap,
    Json(req): Json<ExchangeCodeRequest>,
) -> Result<Json<ApiResp<ExchangeCodeResponse>>> {
    let auth_service = state.auth_service().ok_or_else(|| {
        Error::InternalError("认证服务未初始化".to_string())
    })?;

    // 提取设备信息（前端直调模式签发 TokenPair 时使用）
    let device_info = extract_device_info(&headers);

    let exchange_result = auth_service.exchange_oauth2_callback_code(&req.code, &req.state, device_info).await.map_err(|e| {
        error!(error = %e, "授权码换 Token 失败");
        match e {
            cmx_traits::auth::AuthError::OAuth2CallbackCodeInvalid => {
                Error::Unauthorized("授权码或 state 无效或已过期".to_string())
            }
            // state 不匹配属于 CSRF 攻击迹象，返回 401 Unauthorized
            cmx_traits::auth::AuthError::OAuth2(msg) if msg.contains("不匹配") => {
                Error::Unauthorized(msg)
            }
            cmx_traits::auth::AuthError::OAuth2(msg) => {
                Error::BadRequest(msg)
            }
            cmx_traits::auth::AuthError::OAuth2ProviderNotFound(_) => {
                Error::BadRequest("Provider 不存在".to_string())
            }
            cmx_traits::auth::AuthError::OAuth2ProviderUnavailable(_) => {
                Error::BadRequest("Provider 服务不可用".to_string())
            }
            cmx_traits::auth::AuthError::OAuth2ProviderTokenError(_) => {
                Error::BadRequest("Provider 授权失败".to_string())
            }
            cmx_traits::auth::AuthError::OAuth2ProviderUserInfoError(_) => {
                Error::BadRequest("Provider 用户信息获取失败".to_string())
            }
            other => Error::InternalError(other.to_string()),
        }
    })?;

    // 校验 state 一致性（防 CSRF 二次防护）
    if !req.state.is_empty() && req.state != exchange_result.state {
        return Err(Error::Unauthorized("state 不匹配".to_string()));
    }

    let response = ExchangeCodeResponse {
        access_token: exchange_result.access_token,
        refresh_token: exchange_result.refresh_token,
        token_type: exchange_result.token_type,
        access_expires_at: exchange_result.access_expires_at,
        refresh_expires_at: exchange_result.refresh_expires_at,
        is_new: exchange_result.is_new,
        provider: exchange_result.provider,
        state: exchange_result.state,
    };

    Ok(Json(ApiResp::ok(response)))
}

/// 手动绑定第三方账号请求载荷。
///
/// 已登录用户可通过此接口将第三方 OAuth2 账号绑定到当前用户，绑定后
/// 可使用第三方登录方式直接进入该账号。
#[derive(Debug, Deserialize)]
pub struct LinkAccountRequest {
    /// Provider 返回的授权码。
    pub code: String,
}

/// 绑定第三方 OAuth2 账号到已登录用户
pub async fn oauth2_provider_link(
    State(state): State<CmxAppState>,
    Path(provider): Path<String>,
    CmxSvrContext(svr_ctx): CmxSvrContext,
    Json(req): Json<LinkAccountRequest>,
) -> Result<Json<ApiResp<()>>> {
    let auth_ctx = svr_ctx.auth_context.ok_or_else(|| {
        Error::Unauthorized("未认证".to_string())
    })?;

    let auth_service = state.auth_service().ok_or_else(|| {
        Error::InternalError("认证服务未初始化".to_string())
    })?;

    auth_service.link_oauth2_account(&auth_ctx.user_id, &provider, &req.code).await.map_err(|e| {
        error!(user_id = %auth_ctx.user_id, provider = %provider, error = %e, "绑定第三方账号失败");
        match e {
            cmx_traits::auth::AuthError::OAuth2ProviderNotFound(_) => Error::BadRequest("Provider 不存在".to_string()),
            cmx_traits::auth::AuthError::OAuth2ProviderUnavailable(_) => Error::BadRequest("Provider 服务不可用".to_string()),
            cmx_traits::auth::AuthError::OAuth2ProviderTokenError(_) => Error::BadRequest("Provider 授权失败".to_string()),
            cmx_traits::auth::AuthError::OAuth2ProviderUserInfoError(_) => Error::BadRequest("Provider 用户信息获取失败".to_string()),
            // N-11 修复：仅匹配"已被其他用户绑定"场景，其他 OAuth2 错误走 InternalError
            cmx_traits::auth::AuthError::OAuth2(msg) if msg.contains("已被其他用户绑定") => {
                Error::BadRequest("该第三方账号已被其他用户绑定".to_string())
            }
            other => Error::InternalError(other.to_string()),
        }
    })?;

    Ok(Json(ApiResp::msg("绑定成功")))
}

/// 解除第三方 OAuth2 账号绑定
pub async fn oauth2_provider_unlink(
    State(state): State<CmxAppState>,
    Path(provider): Path<String>,
    CmxSvrContext(svr_ctx): CmxSvrContext,
) -> Result<Json<ApiResp<()>>> {
    let auth_ctx = svr_ctx.auth_context.ok_or_else(|| {
        Error::Unauthorized("未认证".to_string())
    })?;

    let auth_service = state.auth_service().ok_or_else(|| {
        Error::InternalError("认证服务未初始化".to_string())
    })?;

    auth_service.unlink_oauth2_account(&auth_ctx.user_id, &provider).await.map_err(|e| {
        error!(user_id = %auth_ctx.user_id, provider = %provider, error = %e, "解绑第三方账号失败");
        match e {
            cmx_traits::auth::AuthError::OAuth2LastBindingCannotRemove => {
                Error::BadRequest("无法解除最后一个登录绑定".to_string())
            }
            cmx_traits::auth::AuthError::OAuth2ProviderNotFound(_) => Error::BadRequest(e.to_string()),
            other => Error::InternalError(other.to_string()),
        }
    })?;

    Ok(Json(ApiResp::msg("解绑成功")))
}

/// 获取前端回调 URL（从配置读取，配置缺失时返回错误）
fn get_frontend_callback_url() -> Result<String> {
    cmx_utils::ConfigManager::global()
        .get_string("auth.oauth2.frontend_callback_url")
        .map_err(|_| Error::InternalError("未配置 auth.oauth2.frontend_callback_url".to_string()))
}

/// 将 AuthError 脱敏为前端友好的错误码，避免泄露内部信息
fn sanitize_oauth2_error(e: &cmx_traits::auth::AuthError) -> &'static str {
    match e {
        cmx_traits::auth::AuthError::OAuth2ProviderNotFound(_) => "provider_not_found",
        cmx_traits::auth::AuthError::OAuth2ProviderUnavailable(_) => "provider_unavailable",
        cmx_traits::auth::AuthError::OAuth2ProviderTokenError(_) => "provider_token_error",
        cmx_traits::auth::AuthError::OAuth2ProviderUserInfoError(_) => "provider_userinfo_error",
        cmx_traits::auth::AuthError::OAuth2AccountNotLinked { .. } => "account_not_registered",
        cmx_traits::auth::AuthError::OAuth2EmailNotVerified => "email_not_verified",
        cmx_traits::auth::AuthError::OAuth2LastBindingCannotRemove => "last_binding_cannot_remove",
        cmx_traits::auth::AuthError::OAuth2UsernameConflict(_) => "username_conflict",
        cmx_traits::auth::AuthError::OAuth2CallbackCodeInvalid => "callback_code_invalid",
        cmx_traits::auth::AuthError::UserDisabled => "user_disabled",
        cmx_traits::auth::AuthError::OAuth2(_) => "authentication_failed",
        _ => "internal_error",
    }
}

/// 从请求头提取设备信息
fn extract_device_info(headers: &HeaderMap) -> Option<cmx_traits::auth::DeviceInfo> {
    let device_type = headers.get("X-Device-Type")
        .and_then(|v| v.to_str().ok())
        .map(String::from);
    let device_id = headers.get("X-Device-Id")
        .and_then(|v| v.to_str().ok())
        .map(String::from);
    let ip = headers.get("X-Forwarded-For")
        .and_then(|v| v.to_str().ok())
        .map(String::from);
    let user_agent = headers.get("User-Agent")
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    if device_type.is_none() && ip.is_none() && user_agent.is_none() {
        return None;
    }

    Some(cmx_traits::auth::DeviceInfo {
        device_type: device_type.unwrap_or_else(|| "web".to_string()),
        device_id: device_id.unwrap_or_default(),
        ip,
        user_agent,
    })
}
