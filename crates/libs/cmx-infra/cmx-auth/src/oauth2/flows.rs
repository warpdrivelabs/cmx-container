//! OAuth2 Authorization Code Flow 实现
//!
//! 实现 authorize → login → token 三步授权码流程（RFC 6749）+ PKCE 扩展（RFC 7636）。
//! N11+N12 修正：授权码主 key 改为 auth:oauth2:authcode:{code}，
//! 换 token 时用 code 反查，补充 client_id + redirect_uri 一致性校验。

use cmx_buffer::CacheManager;
use cmx_traits::AuthError;
use uuid::Uuid;

use crate::config::AuthConfig;
use crate::oauth2::pkce::PkceVerifier;
use crate::oauth2::store::{AuthorizationCode, OAuth2Client, OAuth2Store};

/// 规范化 redirect_uri（移除尾部斜杠、排序 query 参数）
fn normalize_redirect_uri(uri: &str) -> String {
    uri.trim_end_matches('/').to_string()
}

/// OAuth2 授权请求参数
#[derive(Debug, Clone)]
pub struct AuthorizeParams {
    /// 客户端 ID
    pub client_id: String,
    /// 回调地址
    pub redirect_uri: String,
    /// 响应类型（固定 "code"）
    pub response_type: String,
    /// PKCE code_challenge
    pub code_challenge: Option<String>,
    /// PKCE code_challenge_method
    pub code_challenge_method: Option<String>,
    /// 请求的 scope
    pub scope: Vec<String>,
    /// CSRF state
    pub state: String,
}

/// OAuth2 Token 交换请求参数
#[derive(Debug, Clone)]
pub struct TokenExchangeParams {
    /// 授权码
    pub code: String,
    /// PKCE code_verifier
    pub code_verifier: Option<String>,
    /// 客户端 ID
    pub client_id: String,
    /// 回调地址（N12：一致性校验）
    pub redirect_uri: String,
}

/// OAuth2 Flow 服务
#[derive(Clone)]
pub struct OAuth2FlowService {
    store: OAuth2Store,
    config: AuthConfig,
}

impl OAuth2FlowService {
    /// 创建新的 OAuth2 Flow 服务
    pub fn new(cache: CacheManager, config: AuthConfig) -> Self {
        let store = OAuth2Store::new(cache, config.clone());
        Self { store, config }
    }

    /// 第一步：authorize — 验证客户端并存储 CSRF state
    pub async fn authorize(
        &self,
        params: AuthorizeParams,
        client: &OAuth2Client,
    ) -> Result<String, AuthError> {
        // 1. 验证客户端状态
        if client.status == 0 {
            return Err(AuthError::OAuth2("客户端已禁用".to_string()));
        }

        // 2. 验证 redirect_uri（规范化比较）
        let normalized_redirect = normalize_redirect_uri(&params.redirect_uri);
        let registered_uris: Vec<String> = client.redirect_uris.iter()
            .map(|u| normalize_redirect_uri(u))
            .collect();
        if !registered_uris.contains(&normalized_redirect) {
            return Err(AuthError::OAuth2("redirect_uri 未注册".to_string()));
        }

        // 3. PKCE 强制校验
        let pkce_required = self
            .config
            .oauth2
            .as_ref()
            .map(|c| c.pkce_required)
            .unwrap_or(true);

        if pkce_required && params.code_challenge.is_none() {
            return Err(AuthError::PkceVerificationFailed);
        }

        // 4. 存储 CSRF state
        self.store
            .store_csrf_state(&params.state, &params.client_id)
            .await
            .map_err(|e| AuthError::Internal(e.to_string()))?;

        Ok(params.state)
    }

    /// 第二步：login — 用户认证后签发授权码
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
        // 1. 读取 CSRF state（不消费）
        let stored_client_id = self
            .store
            .get_csrf_state(state)
            .await
            .map_err(|e| AuthError::Internal(e.to_string()))?;

        match stored_client_id {
            Some(stored) if stored == client_id => {
                // 验证成功后才消费
                self.store
                    .consume_csrf_state(state)
                    .await
                    .map_err(|e| AuthError::Internal(e.to_string()))?;
            }
            Some(_) => {
                return Err(AuthError::OAuth2("state 与 client_id 不匹配".to_string()));
            }
            None => {
                return Err(AuthError::OAuth2("state 无效或已过期".to_string()));
            }
        }

        // 2. 生成授权码
        let code = Uuid::new_v4().to_string().replace("-", "");

        let auth_code = AuthorizationCode {
            code: code.clone(),
            client_id: client_id.to_string(),
            user_id: Some(user_id.to_string()),
            redirect_uri: redirect_uri.to_string(),
            code_challenge,
            code_challenge_method,
            scope,
            state: state.to_string(),
            approved: true,
            created_at: chrono::Utc::now().timestamp(),
        };

        // 3. 存储授权码
        self.store
            .store_authorization_code(&auth_code)
            .await
            .map_err(|e| AuthError::Internal(e.to_string()))?;

        Ok(code)
    }

    /// 第三步：token — 用授权码换 Token
    ///
    /// N12 修正：校验 client_id + redirect_uri 一致性。
    pub async fn exchange_code(
        &self,
        params: TokenExchangeParams,
    ) -> Result<AuthorizationCode, AuthError> {
        // 1. 获取并消费授权码
        let auth_code = self
            .store
            .consume_authorization_code(&params.code)
            .await
            .map_err(|e| AuthError::Internal(e.to_string()))?;

        let auth_code = match auth_code {
            Some(ac) => ac,
            None => return Err(AuthError::InvalidAuthCode),
        };

        // 2. 检查授权码是否已授权
        if !auth_code.approved {
            return Err(AuthError::InvalidAuthCode);
        }

        // 3. N12：校验 client_id 一致性（防跨客户端劫持）
        if auth_code.client_id != params.client_id {
            return Err(AuthError::OAuth2("client_id 不匹配".to_string()));
        }

        // 4. N12：校验 redirect_uri 一致性（RFC 6749 §4.1.3）
        if auth_code.redirect_uri != params.redirect_uri {
            return Err(AuthError::OAuth2("redirect_uri 不匹配".to_string()));
        }

        // 5. PKCE 校验
        if let Some(ref challenge) = auth_code.code_challenge {
            let method = auth_code
                .code_challenge_method
                .as_deref()
                .unwrap_or("S256");

            match &params.code_verifier {
                Some(verifier) => {
                    if !PkceVerifier::verify(verifier, challenge, method) {
                        return Err(AuthError::PkceVerificationFailed);
                    }
                }
                None => return Err(AuthError::PkceVerificationFailed),
            }
        }

        // 6. 检查用户是否已授权
        if auth_code.user_id.is_none() {
            return Err(AuthError::OAuth2("授权码未关联用户".to_string()));
        }

        Ok(auth_code)
    }
}
