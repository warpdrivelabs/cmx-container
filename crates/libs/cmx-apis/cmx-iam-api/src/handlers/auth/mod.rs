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
fn inner_routes() -> Router<CmxAppState> {
    Router::new()
        .route("/login", post(auth_login))
        .route("/refresh", post(auth_refresh))
        .route("/logout", post(auth_logout))
        .route("/me", get(auth_me))
        .route("/validate", post(auth_validate))
        .route("/revoke-all", post(auth_revoke_all))
        .route("/heartbeat", post(auth_heartbeat))
        .route("/change-password", post(auth_change_password))
        .route("/health", get(auth_health))
        // OAuth2 路由
        .route("/oauth2/authorize", get(oauth2_handler::oauth2_authorize))
        .route("/oauth2/login", post(oauth2_handler::oauth2_login))
        .route("/oauth2/token", post(oauth2_handler::oauth2_token))
        // 第三方 OAuth2 Provider 路由
        .route(
            "/oauth2/providers",
            get(oauth2_provider_handler::oauth2_providers),
        )
        .route(
            "/oauth2/provider/{provider}/authorize",
            get(oauth2_provider_handler::oauth2_provider_authorize),
        )
        .route(
            "/oauth2/{provider}/callback",
            get(oauth2_provider_handler::oauth2_provider_callback),
        )
        .route(
            "/oauth2/provider/exchange",
            post(oauth2_provider_handler::oauth2_provider_exchange),
        )
        .route(
            "/oauth2/provider/{provider}/link",
            post(oauth2_provider_handler::oauth2_provider_link),
        )
        .route(
            "/oauth2/provider/{provider}/unlink",
            delete(oauth2_provider_handler::oauth2_provider_unlink),
        )
        // API Key 管理路由
        .route("/api-keys/create", post(api_key_handler::create_api_key))
        .route("/api-keys/list", get(api_key_handler::list_api_keys))
        .route("/api-keys/delete", post(api_key_handler::delete_api_key))
        .route(
            "/api-keys/toggle-status",
            post(api_key_handler::toggle_api_key_status),
        )
        // OAuth2 客户端管理路由
        .route(
            "/oauth2-clients/create",
            post(oauth2_client_handler::create_oauth2_client),
        )
        .route(
            "/oauth2-clients/list",
            get(oauth2_client_handler::list_oauth2_clients),
        )
        .route(
            "/oauth2-clients/update",
            post(oauth2_client_handler::update_oauth2_client_by_id),
        )
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
