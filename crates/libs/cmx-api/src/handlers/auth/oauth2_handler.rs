//! OAuth2 Handler 实现
//!
//! 提供 authorize / login / token 三个 OAuth2 Authorization Code Flow API。

use axum::extract::{Query, State};
use axum::Json;
use cmx_traits::auth::Credentials;
use tracing::{debug, info};

use crate::middleware::GlobalAuthService;
use crate::{ApiResp, Error, Result};
use crate::app_state::CmxAppState;
use crate::middleware::CmxSvrContext;

use super::oauth2_request::*;
use super::oauth2_response::*;

/// OAuth2 authorize — 验证客户端并存储 CSRF state
#[utoipa::path(
    get,
    path = "/api/auth/oauth2/authorize",
    params(OAuth2AuthorizeRequest),
    responses(
        (status = 200, description = "验证成功", body = OAuth2AuthorizeResponse),
        (status = 400, description = "参数错误"),
        (status = 401, description = "客户端未注册或已禁用")
    ),
    tag = "OAuth2"
)]
pub async fn oauth2_authorize(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Query(req): Query<OAuth2AuthorizeRequest>,
) -> Result<Json<ApiResp<OAuth2AuthorizeResponse>>> {
    debug!("{:<12} - handler::oauth2_authorize - client_id: {}", "HANDLER", req.client_id);

    let auth_service = cmx_state.auth_service().ok_or_else(|| {
        Error::InternalError("认证服务未初始化".to_string())
    })?;

    let oauth2_policy = GlobalAuthService::get_oauth2().ok_or_else(|| {
        Error::InternalError("OAuth2 服务未初始化".to_string())
    })?;

    // 从数据库查询 OAuth2 客户端
    let oauth2_client_data = auth_service
        .get_oauth2_client(&req.client_id)
        .await
        .map_err(|e| Error::InternalError(e.to_string()))?
        .ok_or_else(|| Error::BadRequest("客户端未注册".to_string()))?;

    // 检查客户端状态
    if oauth2_client_data.status == 0 {
        return Err(Error::BadRequest("客户端已禁用".to_string()));
    }

    let client = cmx_auth::oauth2::store::OAuth2Client::from(oauth2_client_data);

    let scope: Vec<String> = req.scope
        .as_deref()
        .map(|s| s.split_whitespace().map(String::from).collect())
        .unwrap_or_default();

    let state = oauth2_policy
        .authorize(
            &client,
            req.redirect_uri.clone(),
            req.code_challenge.clone(),
            req.code_challenge_method.clone(),
            scope,
            req.state.clone(),
        )
        .await
        .map_err(|e| match e {
            cmx_traits::auth::AuthError::OAuth2(msg) => Error::BadRequest(msg),
            cmx_traits::auth::AuthError::PkceVerificationFailed => {
                Error::BadRequest("PKCE code_challenge 必填".to_string())
            }
            other => Error::InternalError(other.to_string()),
        })?;

    let response = OAuth2AuthorizeResponse { state };
    Ok(Json(ApiResp::ok(response)))
}

/// OAuth2 login — 用户认证后签发授权码
#[utoipa::path(
    post,
    path = "/api/auth/oauth2/login",
    request_body = OAuth2LoginRequest,
    responses(
        (status = 200, description = "登录成功", body = OAuth2LoginResponse),
        (status = 401, description = "用户名或密码错误")
    ),
    tag = "OAuth2"
)]
pub async fn oauth2_login(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Json(req): Json<OAuth2LoginRequest>,
) -> Result<Json<ApiResp<OAuth2LoginResponse>>> {
    debug!("{:<12} - handler::oauth2_login - client_id: {}", "HANDLER", req.client_id);

    // 1. 验证用户名密码，仅获取 user_id（2.4 修复：不再签发又撤销 Token）
    let auth_service = cmx_state.auth_service().ok_or_else(|| {
        Error::InternalError("认证服务未初始化".to_string())
    })?;

    let user_id = auth_service
        .verify_credentials(&req.username, &req.password)
        .await
        .map_err(|e| match e {
            cmx_traits::auth::AuthError::InvalidCredentials => {
                Error::Unauthorized("用户名或密码错误".to_string())
            }
            cmx_traits::auth::AuthError::UserDisabled => {
                Error::Forbidden("用户已被禁用".to_string())
            }
            other => Error::InternalError(other.to_string()),
        })?;

    // 2. 用 OAuth2 策略签发授权码
    let oauth2_policy = GlobalAuthService::get_oauth2().ok_or_else(|| {
        Error::InternalError("OAuth2 服务未初始化".to_string())
    })?;

    let scope: Vec<String> = req.scope
        .as_deref()
        .map(|s| s.split_whitespace().map(String::from).collect())
        .unwrap_or_default();

    let code = oauth2_policy
        .login(
            &req.state,
            &user_id,
            &req.client_id,
            &req.redirect_uri,
            req.code_challenge,
            req.code_challenge_method,
            scope,
        )
        .await
        .map_err(|e| match e {
            cmx_traits::auth::AuthError::OAuth2(msg) => Error::BadRequest(msg),
            other => Error::InternalError(other.to_string()),
        })?;

    info!(client_id = %req.client_id, "OAuth2 授权码签发成功");

    let response = OAuth2LoginResponse {
        code,
        state: req.state,
    };
    Ok(Json(ApiResp::ok(response)))
}

/// OAuth2 token — 用授权码换 Token
#[utoipa::path(
    post,
    path = "/api/auth/oauth2/token",
    request_body = OAuth2TokenRequest,
    responses(
        (status = 200, description = "Token 签发成功", body = OAuth2TokenResponse),
        (status = 400, description = "授权码无效或已过期")
    ),
    tag = "OAuth2"
)]
pub async fn oauth2_token(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Json(req): Json<OAuth2TokenRequest>,
) -> Result<Json<ApiResp<OAuth2TokenResponse>>> {
    debug!("{:<12} - handler::oauth2_token - client_id: {}", "HANDLER", req.client_id);

    let auth_service = cmx_state.auth_service().ok_or_else(|| {
        Error::InternalError("认证服务未初始化".to_string())
    })?;

    // 使用 AuthorizationCode 凭证签发 Token
    let code_verifier = req.code_verifier.unwrap_or_default();
    let credentials = Credentials::AuthorizationCode {
        code: req.code,
        code_verifier,
        client_id: req.client_id,
    };

    let token_pair = auth_service
        .authenticate(credentials, None)
        .await
        .map_err(|e| match e {
            cmx_traits::auth::AuthError::InvalidAuthCode => {
                Error::BadRequest("授权码无效或已过期".to_string())
            }
            cmx_traits::auth::AuthError::PkceVerificationFailed => {
                Error::BadRequest("PKCE 校验失败".to_string())
            }
            cmx_traits::auth::AuthError::OAuth2(msg) => {
                Error::BadRequest(msg)
            }
            cmx_traits::auth::AuthError::InvalidToken(msg) => {
                Error::Unauthorized(msg)
            }
            other => Error::InternalError(other.to_string()),
        })?;

    let now = chrono::Utc::now().timestamp();
    let response = OAuth2TokenResponse {
        access_token: token_pair.access_token,
        refresh_token: token_pair.refresh_token,
        token_type: token_pair.token_type,
        expires_in: token_pair.access_expires_at - now,
        refresh_expires_in: token_pair.refresh_expires_at - now,
    };

    Ok(Json(ApiResp::ok(response)))
}
