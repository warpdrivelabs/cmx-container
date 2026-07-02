//! 第三方 OAuth2 登录与回调
//!
//! 实现 [`cmx_traits::auth::AuthService`] 的第三方 OAuth2 全流程：登录、回调处理、
//! 统一 exchange、账号绑定/解绑、Provider state 存储与列表查询。
//! 同时包含 OAuth2 核心流程辅助方法（code 交换、用户关联、Token 签发）。

use cmx_traits::auth::{
    AuthError, DeviceInfo, OAuth2CallbackExchangeResult, OAuth2CallbackResult, TokenPair,
};
use tracing::info;

use crate::auth_service_impl::AuthServiceImpl;

/// 回调授权码存储数据（包含 TokenPair + is_new + provider + state）。
#[derive(serde::Serialize, serde::Deserialize)]
pub(super) struct CallbackCodeData {
    pub(super) access_token: String,
    pub(super) refresh_token: String,
    pub(super) token_type: String,
    pub(super) access_expires_at: i64,
    pub(super) refresh_expires_at: i64,
    pub(super) is_new: bool,
    pub(super) provider: String,
    pub(super) state: String,
}

impl AuthServiceImpl {
    /// 第三方 OAuth2 登录认证
    pub(super) async fn authenticate_third_party(
        &self,
        user_id: &str,
        provider: &str,
        provider_user_id: &str,
        device_info: Option<DeviceInfo>,
    ) -> std::result::Result<TokenPair, AuthError> {
        // 1. 验证用户存在且启用
        let user = self
            .user_query
            .get_user_by_id(user_id)
            .await
            .map_err(|e| AuthError::Internal(e.to_string()))?
            .ok_or(AuthError::OAuth2AccountNotLinked {
                provider: provider.to_string(),
                provider_user_id: provider_user_id.to_string(),
            })?;

        if user.status == 0 {
            return Err(AuthError::UserDisabled);
        }

        // 2. 获取角色和权限
        let roles = self
            .user_query
            .get_user_role_codes(user_id)
            .await
            .map_err(|e| AuthError::Internal(e.to_string()))?;
        let permissions = self
            .user_query
            .get_user_permissions(user_id)
            .await
            .map_err(|e| AuthError::Internal(e.to_string()))?;

        // 3. 签发 TokenPair（org_id 传 None，与现有密码/APIKey/OAuth2 分支一致）
        let result = self
            .issue_token_pair(
                user_id,
                &user.username,
                &roles,
                &permissions,
                None,
                device_info.as_ref(),
            )
            .await;

        if result.is_ok() {
            crate::metrics::record_login_success("third_party_oauth2");
            info!(provider = %provider, user_id = %user_id, "第三方 OAuth2 登录成功");
        }

        result
    }

    /// 处理 Provider 授权码的核心逻辑（不含 state 消费）。
    ///
    /// 前置条件：state 已由调用方消费，并校验 provider 一致性。
    ///
    /// 供 `handle_oauth2_callback`（后端回调模式）和 `exchange_oauth2_callback_code`
    /// （统一 exchange，前端直调分支）复用。
    pub(super) async fn process_provider_code_after_state(
        &self,
        provider: &str,
        code: &str,
        device_info: Option<DeviceInfo>,
    ) -> std::result::Result<(cmx_traits::auth::TokenPair, bool, String), AuthError> {
        // 1. 获取 Provider
        let registry = crate::oauth2::OAuth2ProviderRegistry::get_global().ok_or(
            AuthError::Internal("OAuth2 Provider 注册表未初始化".to_string()),
        )?;
        let provider_impl = registry.get_provider(provider)?;

        // 2. 获取 redirect_uri（从 Provider 配置中获取）
        let redirect_uri = provider_impl.redirect_uri().to_string();

        // 3. 交换 Token
        let token_response = provider_impl.exchange_code(code, &redirect_uri).await?;
        tracing::info!(provider = %provider, "Token 交换成功");

        // 4. 获取用户信息
        let user_info = provider_impl.get_user_info(&token_response).await?;
        tracing::info!(provider = %provider, provider_user_id = %user_info.provider_user_id, "用户信息获取成功");

        // 5. 关联/注册用户
        let link_result = self
            .account_linker
            .find_or_link(provider, &user_info.provider_user_id, &user_info)
            .await?;

        let (user_id, is_new) = match link_result {
            crate::oauth2::provider::LinkResult::Linked { user_id, is_new } => (user_id, is_new),
            crate::oauth2::provider::LinkResult::BindingRequired { .. } => {
                return Err(AuthError::OAuth2(
                    "账号未注册，请联系管理员开通".to_string(),
                ));
            }
        };

        // 6. 签发本平台 Token
        let token_pair = self
            .authenticate(
                cmx_traits::auth::Credentials::ThirdPartyOAuth2 {
                    provider: provider.to_string(),
                    provider_user_id: user_info.provider_user_id,
                    user_id: user_id.clone(),
                },
                device_info,
            )
            .await?;

        // 7. 审计日志：第三方 OAuth2 登录
        self.audit_log(
            "oauth2_login",
            cmx_audit::OperationResult::Success,
            &user_id,
            Some("user"),
            Some(&user_id),
            Some(serde_json::json!({
                "provider": provider,
                "is_new": is_new,
            })),
        )
        .await;

        Ok((token_pair, is_new, user_id))
    }

    /// 列出所有已启用的第三方 OAuth2 Provider 信息。
    ///
    /// # Returns
    ///
    /// 返回 `Vec<ProviderInfo>`。注册表未初始化时返回空列表，
    /// 使公开端点 `GET /api/auth/oauth2/providers` 优雅返回空数组。
    pub(super) async fn list_oauth2_providers(
        &self,
    ) -> std::result::Result<Vec<cmx_traits::auth::ProviderInfo>, AuthError> {
        // 未配置任何第三方 Provider 时（注册表未初始化），优雅返回空列表而非报错，
        // 使公开端点 GET /api/auth/oauth2/providers 返回 {code:0,data:[]}。
        match crate::oauth2::OAuth2ProviderRegistry::get_global() {
            Some(registry) => Ok(registry.list_providers()),
            None => Ok(Vec::new()),
        }
    }

    /// 处理第三方 OAuth2 回调。
    ///
    /// 流程：原子消费 state → 执行核心流程（换 token、获取用户信息、关联用户、
    /// 签发 TokenPair）→ 存储一次性回调授权码（前端用 code 换取 TokenPair）。
    ///
    /// # Arguments
    ///
    /// * `provider` - 第三方 Provider 名称。
    /// * `code` - 第三方 Provider 返回的授权码。
    /// * `state` - CSRF state（与 `store_oauth2_provider_state` 时存储的对应）。
    /// * `device_info` - 设备信息（可选）。
    ///
    /// # Returns
    ///
    /// 包含一次性 `callback_code` 的 `OAuth2CallbackResult`，前端用它换取 TokenPair。
    ///
    /// # Errors
    ///
    /// * `AuthError::OAuth2` - state 无效/provider 不匹配/账号未注册等。
    /// * `AuthError::OAuth2ProviderUnavailable` - 第三方服务不可达。
    pub(super) async fn handle_oauth2_callback(
        &self,
        provider: &str,
        code: &str,
        state: &str,
        device_info: Option<DeviceInfo>,
    ) -> std::result::Result<OAuth2CallbackResult, AuthError> {
        // 1. 原子消费 state，获取 provider 名称
        let stored_provider = self
            .oauth2_store
            .consume_provider_state(state)
            .await
            .map_err(|e| AuthError::Internal(e.to_string()))?
            .ok_or(AuthError::OAuth2(
                "OAuth2 Provider state 无效或已过期".to_string(),
            ))?;

        if stored_provider != provider {
            return Err(AuthError::OAuth2(
                "State 中的 provider 与请求不匹配".to_string(),
            ));
        }

        // 2. 执行核心流程（换 token、获取用户信息、关联用户、签发 TokenPair）
        let (token_pair, is_new, _user_id) = self
            .process_provider_code_after_state(provider, code, device_info)
            .await?;

        // 3. 签发一次性回调授权码（存储 TokenPair + is_new + provider）
        let callback_code = uuid::Uuid::new_v4().to_string();
        let callback_data = CallbackCodeData {
            access_token: token_pair.access_token,
            refresh_token: token_pair.refresh_token,
            token_type: token_pair.token_type,
            access_expires_at: token_pair.access_expires_at,
            refresh_expires_at: token_pair.refresh_expires_at,
            is_new,
            provider: provider.to_string(),
            state: state.to_string(),
        };
        let callback_data_json = serde_json::to_string(&callback_data)
            .map_err(|e| AuthError::Internal(e.to_string()))?;
        self.oauth2_store
            .store_callback_code(&callback_code, &callback_data_json)
            .await
            .map_err(|e| AuthError::Internal(e.to_string()))?;

        Ok(cmx_traits::auth::OAuth2CallbackResult {
            callback_code,
            state: state.to_string(),
            is_new,
            provider: provider.to_string(),
        })
    }

    /// 用授权码交换 TokenPair 及附加信息（统一接口，支持两种模式）。
    ///
    /// **后端回调模式**：`code` 为 `handle_oauth2_callback` 签发的一次性回调码，
    /// 后端从 Redis 取回已签发的 TokenPair 返回。
    ///
    /// **前端直调模式**：`code` 为 Provider 返回的原始授权码，后端消费 `state`
    /// 获取 provider，执行完整的换 token + 用户关联 + 签发 TokenPair 流程。
    ///
    /// 后端自动判断模式，前端无感知。
    ///
    /// # Arguments
    ///
    /// * `code` - 一次性回调授权码（后端回调模式）或 Provider 原始授权码（前端直调模式）。
    /// * `state` - 原始 state（用于 CSRF 校验 + 前端直调模式下消费获取 provider）。
    /// * `device_info` - 设备信息（前端直调模式签发 TokenPair 时使用）。
    ///
    /// # Returns
    ///
    /// 成功时返回 `OAuth2CallbackExchangeResult`，包含 TokenPair 与元信息。
    ///
    /// # Errors
    ///
    /// * `AuthError::OAuth2CallbackCodeInvalid` - 授权码和 state 均无效或已过期。
    /// * `AuthError::OAuth2` - state 不匹配 / 账号未注册 / Provider 错误等。
    /// * `AuthError::Internal` - 回调数据反序列化失败。
    pub(super) async fn exchange_oauth2_callback_code(
        &self,
        code: &str,
        state: &str,
        device_info: Option<DeviceInfo>,
    ) -> std::result::Result<OAuth2CallbackExchangeResult, AuthError> {
        // 模式 1：后端回调模式 —— 尝试消费一次性回调码
        if let Some(json) = self
            .oauth2_store
            .consume_callback_code(code)
            .await
            .map_err(|e| AuthError::Internal(e.to_string()))?
        {
            tracing::info!("OAuth2 exchange 命中后端回调模式（callback_code 有效）");
            let callback_data: CallbackCodeData = serde_json::from_str(&json)
                .map_err(|e| AuthError::Internal(format!("回调数据反序列化失败: {}", e)))?;

            // 校验 state 一致性（防 CSRF）
            if !state.is_empty() && state != callback_data.state {
                return Err(AuthError::OAuth2("state 不匹配".to_string()));
            }

            return Ok(OAuth2CallbackExchangeResult {
                access_token: callback_data.access_token,
                refresh_token: callback_data.refresh_token,
                token_type: callback_data.token_type,
                access_expires_at: callback_data.access_expires_at,
                refresh_expires_at: callback_data.refresh_expires_at,
                is_new: callback_data.is_new,
                provider: callback_data.provider,
                state: callback_data.state,
            });
        }

        // 模式 2：前端直调模式 —— 尝试消费 state，获取 provider
        let provider = self
            .oauth2_store
            .consume_provider_state(state)
            .await
            .map_err(|e| AuthError::Internal(e.to_string()))?
            .ok_or(AuthError::OAuth2CallbackCodeInvalid)?;

        tracing::info!(provider = %provider, "OAuth2 exchange 命中前端直调模式（消费 state 获取 provider）");

        // 执行核心流程（换 token、获取用户信息、关联用户、签发 TokenPair）
        let (token_pair, is_new, _user_id) = self
            .process_provider_code_after_state(&provider, code, device_info)
            .await?;

        Ok(OAuth2CallbackExchangeResult {
            access_token: token_pair.access_token,
            refresh_token: token_pair.refresh_token,
            token_type: token_pair.token_type,
            access_expires_at: token_pair.access_expires_at,
            refresh_expires_at: token_pair.refresh_expires_at,
            is_new,
            provider,
            state: state.to_string(),
        })
    }

    /// 将第三方 Provider 账号绑定到已有本地用户。
    ///
    /// 流程：获取 Provider → 交换 Token → 获取用户信息 →
    /// 检查账号未被他人绑定 → 创建关联记录。
    ///
    /// # Arguments
    ///
    /// * `user_id` - 本地用户 ID。
    /// * `provider` - 第三方 Provider 名称。
    /// * `code` - 第三方 Provider 返回的授权码。
    ///
    /// # Errors
    ///
    /// * `AuthError::OAuth2` - 该 Provider 账号已被其他用户绑定。
    /// * `AuthError::OAuth2ProviderUnavailable` - 第三方服务不可达。
    /// * `AuthError::Internal` - 注册表未初始化或数据库写入失败。
    pub(super) async fn link_oauth2_account(
        &self,
        user_id: &str,
        provider: &str,
        code: &str,
    ) -> std::result::Result<(), AuthError> {
        // 1. 获取 Provider
        let registry = crate::oauth2::OAuth2ProviderRegistry::get_global().ok_or(
            AuthError::Internal("OAuth2 Provider 注册表未初始化".to_string()),
        )?;
        let provider_impl = registry.get_provider(provider)?;

        // 2. 交换 Token
        let redirect_uri = provider_impl.redirect_uri().to_string();
        let token_response = provider_impl.exchange_code(code, &redirect_uri).await?;

        // 3. 获取用户信息
        let user_info = provider_impl.get_user_info(&token_response).await?;

        // 4. 检查该 Provider 账号是否已被其他用户绑定
        if self
            .account_linker
            .account_exists(provider, &user_info.provider_user_id)
            .await?
        {
            return Err(AuthError::OAuth2(format!(
                "该 {} 账号已被其他用户绑定",
                provider
            )));
        }

        // 5. 创建关联记录
        self.account_linker
            .create_account(provider, &user_info.provider_user_id, user_id, &user_info)
            .await?;

        // 审计日志：第三方账号绑定
        self.audit_log(
            "oauth2_link",
            cmx_audit::OperationResult::Success,
            user_id,
            Some("user"),
            Some(user_id),
            Some(serde_json::json!({
                "provider": provider,
            })),
        )
        .await;

        tracing::info!(user_id = %user_id, provider = %provider, "第三方账号绑定成功");
        Ok(())
    }

    /// 解绑用户的第三方 Provider 账号。
    ///
    /// 委托 `AccountLinker::unlink_account` 执行，包含安全检查：
    /// 若用户既无密码又无其他第三方绑定，则拒绝解绑最后一个登录方式。
    ///
    /// # Arguments
    ///
    /// * `user_id` - 本地用户 ID。
    /// * `provider` - 待解绑的第三方 Provider 名称。
    ///
    /// # Errors
    ///
    /// * `AuthError::OAuth2LastBindingCannotRemove` - 解绑后用户将无可用登录方式。
    /// * `AuthError::Internal` - 数据库操作失败。
    pub(super) async fn unlink_oauth2_account(
        &self,
        user_id: &str,
        provider: &str,
    ) -> std::result::Result<(), AuthError> {
        self.account_linker
            .unlink_account(user_id, provider)
            .await?;

        // 审计日志：第三方账号解绑
        self.audit_log(
            "oauth2_unlink",
            cmx_audit::OperationResult::Success,
            user_id,
            Some("user"),
            Some(user_id),
            Some(serde_json::json!({
                "provider": provider,
            })),
        )
        .await;

        Ok(())
    }

    /// 存储第三方 OAuth2 Provider 的 CSRF state。
    ///
    /// 在重定向用户到第三方授权页前调用，state 用于回调时校验请求来源，
    /// 防止 CSRF 攻击。
    ///
    /// # Arguments
    ///
    /// * `state` - 随机生成的 CSRF state 字符串。
    /// * `provider` - 关联的 Provider 名称，回调时校验一致性。
    ///
    /// # Errors
    ///
    /// 当 Redis 写入失败时返回 `AuthError::Internal`。
    pub(super) async fn store_oauth2_provider_state(
        &self,
        state: &str,
        provider: &str,
    ) -> std::result::Result<(), AuthError> {
        self.oauth2_store
            .store_provider_state(state, provider)
            .await
            .map_err(|e| AuthError::Internal(e.to_string()))
    }
}
