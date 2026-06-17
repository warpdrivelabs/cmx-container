//! OAuth2 认证策略
//!
//! 实现 AuthPolicy trait 的 OAuth2 授权码策略。

use cmx_buffer::CacheManager;
use cmx_traits::auth::AuthError;

use crate::config::AuthConfig;
use crate::oauth2::flows::{OAuth2FlowService, TokenExchangeParams};
use crate::oauth2::store::OAuth2Client;

/// OAuth2 认证策略。
///
/// 封装 OAuth2 Authorization Code Flow 的 `authorize` / `login` / `token` 三步流程，
/// 作为 `AuthPolicy` trait 的实现供统一调用。
#[derive(Clone)]
pub struct OAuth2Policy {
    /// OAuth2 流程服务（处理 state、code、PKCE 等）。
    flow_service: OAuth2FlowService,
}

impl OAuth2Policy {
    /// 创建新的 OAuth2 策略
    pub fn new(cache: CacheManager, config: AuthConfig) -> Self {
        let flow_service = OAuth2FlowService::new(cache, config);
        Self { flow_service }
    }

    /// 用授权码换取用户 ID 和 scope
    ///
    /// 返回 (user_id, scope)
    pub async fn authenticate(
        &self,
        code: &str,
        code_verifier: &str,
        client_id: &str,
        redirect_uri: &str,
    ) -> Result<(String, Vec<String>), AuthError> {
        let params = TokenExchangeParams {
            code: code.to_string(),
            code_verifier: Some(code_verifier.to_string()),
            client_id: client_id.to_string(),
            redirect_uri: redirect_uri.to_string(),
        };

        let auth_code = self.flow_service.exchange_code(params).await?;

        let user_id = auth_code.user_id.ok_or_else(|| {
            AuthError::OAuth2("授权码未关联用户".to_string())
        })?;

        Ok((user_id, auth_code.scope))
    }

    /// authorize 阶段代理
    pub async fn authorize(
        &self,
        client: &OAuth2Client,
        redirect_uri: String,
        code_challenge: Option<String>,
        code_challenge_method: Option<String>,
        scope: Vec<String>,
        state: String,
    ) -> Result<String, AuthError> {
        let params = crate::oauth2::flows::AuthorizeParams {
            client_id: client.client_id.clone(),
            redirect_uri,
            response_type: "code".to_string(),
            code_challenge,
            code_challenge_method,
            scope,
            state,
        };

        self.flow_service.authorize(params, client).await
    }

    /// login 阶段代理
    pub async fn login(
        &self,
        state: &str,
        user_id: &str,
        client_id: &str,
        redirect_uri: &str,
        code_challenge: Option<String>,
        code_challenge_method: Option<String>,
        scope: Vec<String>,
    ) -> Result<String, AuthError> {
        self.flow_service
            .login(
                state,
                user_id,
                client_id,
                redirect_uri,
                code_challenge,
                code_challenge_method,
                scope,
            )
            .await
    }

    /// 获取 flow_service 引用
    pub fn flow_service(&self) -> &OAuth2FlowService {
        &self.flow_service
    }
}
