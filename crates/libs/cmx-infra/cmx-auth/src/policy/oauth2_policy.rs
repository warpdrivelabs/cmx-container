//! OAuth2 认证策略。
//!
//! 实现 `AuthPolicy` trait 的 OAuth2 授权码策略。

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
    /// 创建新的 OAuth2 策略。
    ///
    /// # Arguments
    ///
    /// * `cache` - Redis 缓存管理器。
    /// * `config` - 认证配置。
    ///
    /// # Returns
    ///
    /// 返回构造完成的 `OAuth2Policy` 实例。
    pub fn new(cache: CacheManager, config: AuthConfig) -> Self {
        let flow_service = OAuth2FlowService::new(cache, config);
        Self { flow_service }
    }

    /// 用授权码换取用户 ID 和 scope。
    ///
    /// # Arguments
    ///
    /// * `code` - 授权码字符串。
    /// * `code_verifier` - PKCE `code_verifier`。
    /// * `client_id` - 客户端 ID。
    /// * `redirect_uri` - 回调地址。
    ///
    /// # Returns
    ///
    /// 成功时返回 `(user_id, scope)` 元组。
    ///
    /// # Errors
    ///
    /// 当授权码无效、未关联用户或 PKCE 校验失败时返回对应 `AuthError`。
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

    /// `authorize` 阶段代理。
    ///
    /// 代理调用 `OAuth2FlowService::authorize`，生成授权码并返回重定向 URL。
    ///
    /// # Arguments
    ///
    /// * `client` - OAuth2 客户端信息。
    /// * `redirect_uri` - 回调地址。
    /// * `code_challenge` - PKCE `code_challenge`（可选）。
    /// * `code_challenge_method` - PKCE `code_challenge_method`（可选）。
    /// * `scope` - 请求的 scope 列表。
    /// * `state` - CSRF state 字符串。
    ///
    /// # Returns
    ///
    /// 成功时返回包含授权码的重定向 URL。
    ///
    /// # Errors
    ///
    /// 当客户端校验失败或 state 写入 Redis 失败时返回对应 `AuthError`。
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

    /// `login` 阶段代理。
    ///
    /// 代理调用 `OAuth2FlowService::login`，验证用户登录后生成授权码。
    ///
    /// # Arguments
    ///
    /// * `state` - CSRF state 字符串。
    /// * `user_id` - 已登录的用户 ID。
    /// * `client_id` - 客户端 ID。
    /// * `redirect_uri` - 回调地址。
    /// * `code_challenge` - PKCE `code_challenge`（可选）。
    /// * `code_challenge_method` - PKCE `code_challenge_method`（可选）。
    /// * `scope` - 请求的 scope 列表。
    ///
    /// # Returns
    ///
    /// 成功时返回授权码字符串。
    ///
    /// # Errors
    ///
    /// 当 state 验证失败或授权码存储失败时返回对应 `AuthError`。
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

    /// 获取 `flow_service` 引用。
    ///
    /// # Returns
    ///
    /// 返回内部 `OAuth2FlowService` 的引用，供外部直接调用底层流程方法。
    pub fn flow_service(&self) -> &OAuth2FlowService {
        &self.flow_service
    }
}
