//! 认证管理 API 模块
//!
//! 提供登录/登出/刷新Token/校验Token/OAuth2/API Key/OAuth2客户端管理等 HTTP API

pub mod api_key_handler;
pub mod handler;
pub mod oauth2_client_handler;
pub mod oauth2_handler;
pub mod oauth2_provider_handler;
pub mod oauth2_request;
pub mod oauth2_response;
pub mod request;
pub mod response;

use axum::Router;
use axum::routing::{delete, get, post};

use cmx_api_core::CmxAppState;
use cmx_api_core::ModuleRoutes;

pub use handler::{
    auth_change_password, auth_health, auth_heartbeat, auth_login, auth_logout, auth_me,
    auth_refresh, auth_revoke_all, auth_validate,
};
pub use request::*;
pub use response::*;

/// 内部路由（不含前缀）
///
/// 所有路由挂在 `/api/auth` 下，覆盖四大块：
/// 1. 账号会话（登录 / token / 登出 / 心跳）
/// 2. 平台内置 OAuth2 授权服务器（作为 IdP 对外发码发 token）
/// 3. 第三方 OAuth2 Provider 接入（作为 RP 对接外部 IdP）
/// 4. API Key 与 OAuth2 客户端管理
fn inner_routes() -> Router<CmxAppState> {
    Router::new()
        // ── 账号会话：登录 / Token / 登出 ──
        // 账号密码登录，签发 access / refresh token
        .route("/login", post(auth_login))
        // 用 refresh token 换新的 access token
        .route("/refresh", post(auth_refresh))
        // 登出并吊销当前 token
        .route("/logout", post(auth_logout))
        // 取当前登录用户信息
        .route("/me", get(auth_me))
        // 校验 token 是否有效（供网关 / 第三方调用）
        .route("/validate", post(auth_validate))
        // 吊销该用户所有 token（强制全端下线）
        .route("/revoke-all", post(auth_revoke_all))
        // 心跳续期（滑动窗口刷新最后活跃时间）
        .route("/heartbeat", post(auth_heartbeat))
        // 修改当前用户密码
        .route("/change-password", post(auth_change_password))
        // 认证服务探活（无需鉴权）
        .route("/health", get(auth_health))
        // ── 平台内置 OAuth2 授权服务器（作为 IdP 对外发码发 token）──
        // 授权码端点：重定向到确认页 / 直接发码
        .route("/oauth2/authorize", get(oauth2_handler::oauth2_authorize))
        // OAuth2 登录提交（账号密码 + 授权确认）
        .route("/oauth2/login", post(oauth2_handler::oauth2_login))
        // token 端点：换 access / refresh token
        .route("/oauth2/token", post(oauth2_handler::oauth2_token))
        // ── 第三方 OAuth2 Provider 接入（作为 RP 对接外部 IdP）──
        // 列出已配置的第三方 Provider
        .route(
            "/oauth2/providers",
            get(oauth2_provider_handler::oauth2_providers),
        )
        // 跳转到指定 Provider 的授权页
        .route(
            "/oauth2/provider/{provider}/authorize",
            get(oauth2_provider_handler::oauth2_provider_authorize),
        )
        // Provider 授权回调（拿 code 换 token）
        .route(
            "/oauth2/{provider}/callback",
            get(oauth2_provider_handler::oauth2_provider_callback),
        )
        // 用第三方 token 换平台 token（账户绑定 / 静默登录）
        .route(
            "/oauth2/provider/exchange",
            post(oauth2_provider_handler::oauth2_provider_exchange),
        )
        // 将当前用户与指定第三方账号建立绑定
        .route(
            "/oauth2/provider/{provider}/link",
            post(oauth2_provider_handler::oauth2_provider_link),
        )
        // 解除当前用户与指定第三方账号的绑定（既有接口，保留 DELETE 方法）
        .route(
            "/oauth2/provider/{provider}/unlink",
            delete(oauth2_provider_handler::oauth2_provider_unlink),
        )
        // ── API Key 管理（程序化访问凭证）──
        // 创建 API Key
        .route("/api-keys/create", post(api_key_handler::create_api_key))
        // 列出当前用户的 API Key
        .route("/api-keys/list", get(api_key_handler::list_api_keys))
        // 删除 API Key
        .route("/api-keys/delete", post(api_key_handler::delete_api_key))
        // 启用 / 禁用 API Key
        .route(
            "/api-keys/toggle-status",
            post(api_key_handler::toggle_api_key_status),
        )
        // ── OAuth2 客户端管理（注册接入本平台的第三方应用）──
        // 注册 OAuth2 客户端
        .route(
            "/oauth2-clients/create",
            post(oauth2_client_handler::create_oauth2_client),
        )
        // 列出 OAuth2 客户端
        .route(
            "/oauth2-clients/list",
            get(oauth2_client_handler::list_oauth2_clients),
        )
        // 更新 OAuth2 客户端配置
        .route(
            "/oauth2-clients/update",
            post(oauth2_client_handler::update_oauth2_client_by_id),
        )
        // 删除 OAuth2 客户端
        .route(
            "/oauth2-clients/delete",
            post(oauth2_client_handler::delete_oauth2_client),
        )
}

/// Auth 模块路由
pub struct AuthModule;

impl ModuleRoutes for AuthModule {
    fn routes(self) -> Router<CmxAppState> {
        Router::new().nest("/auth", inner_routes())
    }

    fn prefix() -> &'static str {
        "auth"
    }

    fn module_name(&self) -> &'static str {
        "auth"
    }
}
