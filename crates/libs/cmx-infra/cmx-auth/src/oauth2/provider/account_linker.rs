//! 第三方账号关联/注册逻辑。
//!
//! 根据 `AccountLinkConfig` 策略处理 OAuth2 登录后的本地账号关联：
//! 已关联直接返回、邮箱匹配自动关联、配置允许时自动注册新用户。

use std::sync::Arc;

use cmx_database::crud::{DbBmc, GenericCrudService};
use cmx_database::{DataSet, DatabaseManager, get_default_db_manager};
use cmx_traits::auth::{AuthError, OAuth2UserInfo, UserAuthQuery};
use modql::field::Fields;
use modql::filter::{OpValInt64, OpValString, OpValsInt64, OpValsString};
use serde::{Deserialize, Serialize};

use super::ProviderUserInfo;
use crate::config::AccountLinkConfig;

/// 第三方 OAuth2 账号关联表 Bmc。
struct OAuth2AccountBmc;
impl DbBmc for OAuth2AccountBmc {
    const TABLE: &'static str = "cmx_auth_oauth2_account";
    const PK_COLUMN: &'static str = "id";
}

/// 第三方 OAuth2 账号关联记录。
#[allow(dead_code)]
struct OAuth2Account {
    id: String,
    user_id: String,
    provider: String,
    provider_user_id: String,
    provider_username: Option<String>,
    provider_email: Option<String>,
    provider_email_verified: Option<bool>,
    provider_display_name: Option<String>,
    provider_avatar_url: Option<String>,
}

/// 创建第三方账号关联的输入结构体。
#[derive(Debug, Clone, Serialize, Deserialize, Fields)]
pub struct OAuth2AccountForCreate {
    pub user_id: String,
    pub provider: String,
    pub provider_user_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_email: Option<String>,
    pub provider_email_verified: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_avatar_url: Option<String>,
    pub status: Option<i32>,
}

/// 第三方账号过滤条件。
#[derive(Debug, Clone, modql::filter::FilterNodes, Deserialize, Default)]
pub struct OAuth2AccountFilter {
    pub provider: Option<OpValsString>,
    pub provider_user_id: Option<OpValsString>,
    pub user_id: Option<OpValsString>,
    pub status: Option<OpValsInt64>,
    /// 归档过滤（0-未归档，1-已归档）
    pub archived: Option<OpValsInt64>,
}

/// 第三方账号关联结果。
pub enum LinkResult {
    /// 关联已有用户。
    Linked { user_id: String, is_new: bool },
    /// 账号未注册（企业场景下 `auto_register=false` 且无邮箱匹配时触发）。
    ///
    /// N-8: 语义已从"需要前端绑定"变更为"未注册错误"，由上层转换为 `AuthError`。
    BindingRequired {
        provider: String,
        provider_user_id: String,
        email: Option<String>,
    },
}

/// 第三方 OAuth2 账号关联/注册逻辑。
///
/// 根据 `AccountLinkConfig` 策略处理 OAuth2 登录：
/// 1. 已关联直接返回；2. 邮箱匹配自动关联；3. 配置允许时自动注册新用户。
pub struct AccountLinker {
    /// 用户认证数据查询 trait 对象。
    user_query: Arc<dyn UserAuthQuery>,

    /// 账号关联策略配置（邮箱匹配、自动注册、用户名生成等）。
    config: AccountLinkConfig,
}

impl AccountLinker {
    /// 创建新的 `AccountLinker` 实例。
    ///
    /// # Arguments
    ///
    /// * `user_query` - 用户认证数据查询 trait 对象。
    /// * `config` - 账号关联策略配置。
    ///
    /// # Returns
    ///
    /// 返回构造完成的 `AccountLinker` 实例。
    pub fn new(user_query: Arc<dyn UserAuthQuery>, config: AccountLinkConfig) -> Self {
        Self { user_query, config }
    }

    /// 获取 `DatabaseManager` 引用。
    fn get_db_manager() -> &'static DatabaseManager {
        get_default_db_manager()
    }

    /// 默认 `db_id`（从 `DatabaseManager` 动态获取，不写死）。
    async fn default_db_id() -> String {
        Self::get_db_manager().get_default_db_id().await
    }

    /// 查询第三方账号是否已关联。
    ///
    /// # Arguments
    ///
    /// * `provider` - Provider 名称（如 `google`、`github`）。
    /// * `provider_user_id` - Provider 侧的用户唯一标识。
    ///
    /// # Returns
    ///
    /// 已关联时返回 `Ok(true)`，否则返回 `Ok(false)`。
    ///
    /// # Errors
    ///
    /// 当数据库查询失败时返回 `AuthError::Internal`。
    pub(crate) async fn account_exists(
        &self,
        provider: &str,
        provider_user_id: &str,
    ) -> Result<bool, AuthError> {
        Ok(self
            .find_account(provider, provider_user_id)
            .await?
            .is_some())
    }

    /// 查询第三方账号关联记录。
    async fn find_account(
        &self,
        provider: &str,
        provider_user_id: &str,
    ) -> Result<Option<OAuth2Account>, AuthError> {
        let filters = Some(vec![OAuth2AccountFilter {
            provider: Some(OpValsString(vec![OpValString::Eq(provider.to_string())])),
            provider_user_id: Some(OpValsString(vec![OpValString::Eq(
                provider_user_id.to_string(),
            )])),
            ..Default::default()
        }]);
        let dataset = GenericCrudService::<OAuth2AccountBmc, OAuth2AccountFilter>::list(
            Self::get_db_manager(),
            &Self::default_db_id().await,
            None,
            filters,
            None,
        )
        .await
        .map_err(|e| AuthError::Internal(e.to_string()))?;

        Ok(extract_oauth2_account(dataset))
    }

    /// 更新关联记录的 `last_login_at`。
    async fn update_last_login_at(&self, account_id: &str) -> Result<(), AuthError> {
        let sql = "UPDATE cmx_auth_oauth2_account SET last_login_at = NOW() WHERE id = $1";
        let params: Vec<cmx_core::model::cell::DataValue> =
            vec![cmx_core::model::cell::DataValue::String(
                account_id.to_string(),
            )];

        Self::get_db_manager()
            .execute_sql_with_datavalues(&Self::default_db_id().await, None, sql, params)
            .await
            .map_err(|e| AuthError::Internal(e.to_string()))?;

        Ok(())
    }

    /// 创建第三方账号关联记录。
    ///
    /// # Arguments
    ///
    /// * `provider` - Provider 名称。
    /// * `provider_user_id` - Provider 侧的用户唯一标识。
    /// * `user_id` - 本地用户 ID。
    /// * `user_info` - Provider 返回的用户信息。
    ///
    /// # Errors
    ///
    /// 当数据库写入失败时返回 `AuthError::Internal`。
    pub(crate) async fn create_account(
        &self,
        provider: &str,
        provider_user_id: &str,
        user_id: &str,
        user_info: &ProviderUserInfo,
    ) -> Result<(), AuthError> {
        let data = OAuth2AccountForCreate {
            user_id: user_id.to_string(),
            provider: provider.to_string(),
            provider_user_id: provider_user_id.to_string(),
            provider_username: user_info.username.clone(),
            provider_email: user_info.email.clone(),
            provider_email_verified: user_info.email_verified,
            provider_display_name: user_info.display_name.clone(),
            provider_avatar_url: user_info.avatar_url.clone(),
            status: Some(1),
        };
        GenericCrudService::<OAuth2AccountBmc>::create(
            Self::get_db_manager(),
            &Self::default_db_id().await,
            None,
            data,
        )
        .await
        .map_err(|e| AuthError::Internal(e.to_string()))?;
        Ok(())
    }

    /// 统计用户绑定的其他第三方 Provider 数量。
    async fn count_other_bindings(
        &self,
        user_id: &str,
        exclude_provider: &str,
    ) -> Result<usize, AuthError> {
        let filters = Some(vec![OAuth2AccountFilter {
            user_id: Some(OpValsString(vec![OpValString::Eq(user_id.to_string())])),
            provider: Some(OpValsString(vec![OpValString::Not(
                exclude_provider.to_string(),
            )])),
            status: Some(OpValsInt64(vec![OpValInt64::Eq(1)])),
            ..Default::default()
        }]);
        let count = GenericCrudService::<OAuth2AccountBmc, OAuth2AccountFilter>::count(
            Self::get_db_manager(),
            &Self::default_db_id().await,
            None,
            filters,
        )
        .await
        .map_err(|e| AuthError::Internal(e.to_string()))?;
        Ok(count as usize)
    }

    /// 删除第三方账号关联记录。
    async fn remove_account(&self, user_id: &str, provider: &str) -> Result<(), AuthError> {
        let filters = Some(vec![OAuth2AccountFilter {
            user_id: Some(OpValsString(vec![OpValString::Eq(user_id.to_string())])),
            provider: Some(OpValsString(vec![OpValString::Eq(provider.to_string())])),
            ..Default::default()
        }]);
        let dataset = GenericCrudService::<OAuth2AccountBmc, OAuth2AccountFilter>::list(
            Self::get_db_manager(),
            &Self::default_db_id().await,
            None,
            filters,
            None,
        )
        .await
        .map_err(|e| AuthError::Internal(e.to_string()))?;

        let schema = dataset.schema.as_ref();
        let ids: Vec<serde_json::Value> = dataset
            .iter()
            .filter_map(|row| row.get_by_name_as::<String>(schema, "id"))
            .map(serde_json::Value::String)
            .collect();

        if !ids.is_empty() {
            GenericCrudService::<OAuth2AccountBmc>::delete(
                Self::get_db_manager(),
                &Self::default_db_id().await,
                None,
                ids,
            )
            .await
            .map_err(|e| AuthError::Internal(e.to_string()))?;
        }
        Ok(())
    }

    /// 查找或关联本地用户。
    ///
    /// 按以下顺序处理：
    /// 1. 已关联直接返回并更新 `last_login_at`；
    /// 2. 邮箱匹配自动关联（要求邮箱已验证）；
    /// 3. 用户名匹配自动关联（企业场景，仅对可信 Provider 启用）；
    /// 4. 配置允许时自动注册新用户；
    /// 5. 不自动注册时返回 `BindingRequired`。
    ///
    /// # Arguments
    ///
    /// * `provider` - Provider 名称。
    /// * `provider_user_id` - Provider 侧的用户唯一标识。
    /// * `user_info` - Provider 返回的用户信息。
    ///
    /// # Returns
    ///
    /// 成功时返回 `LinkResult::Linked`（含 `user_id` 和 `is_new` 标记），
    /// 需手动绑定时返回 `LinkResult::BindingRequired`。
    ///
    /// # Errors
    ///
    /// 当数据库查询/写入失败、用户名冲突重试耗尽时返回对应 `AuthError`。
    pub async fn find_or_link(
        &self,
        provider: &str,
        provider_user_id: &str,
        user_info: &ProviderUserInfo,
    ) -> Result<LinkResult, AuthError> {
        // 1. 查询是否已关联
        if let Some(account) = self.find_account(provider, provider_user_id).await? {
            tracing::info!(provider = %provider, provider_user_id = %provider_user_id, user_id = %account.user_id, "已关联，直接返回");
            // 更新 last_login_at
            if let Err(e) = self.update_last_login_at(&account.id).await {
                tracing::warn!(account_id = %account.id, error = %e, "更新 last_login_at 失败");
            }
            return Ok(LinkResult::Linked {
                user_id: account.user_id,
                is_new: false,
            });
        }

        // 2. 自动关联策略（根据邮箱匹配）
        // N-9 降级处理：邮箱未验证时跳过邮箱关联，继续尝试自动注册或返回未注册错误
        if self.config.auto_link_by_email
            && let Some(email) = &user_info.email
        {
            if user_info.email_verified != Some(true) {
                tracing::warn!(provider = %provider, email = %email, "Provider 邮箱未验证，跳过邮箱自动关联");
            } else {
                let user = self
                    .user_query
                    .get_user_by_email(email)
                    .await
                    .map_err(|e| AuthError::Internal(e.to_string()))?;
                if let Some(user) = user {
                    tracing::info!(provider = %provider, email = %email, user_id = %user.user_id, "邮箱匹配，自动关联");
                    self.create_account(provider, provider_user_id, &user.user_id, user_info)
                        .await?;
                    return Ok(LinkResult::Linked {
                        user_id: user.user_id,
                        is_new: false,
                    });
                }
            }
        }

        // 3. 用户名自动关联（企业场景，仅对可信 Provider 启用）
        // 注意：与邮箱关联不同，username 无"已验证"概念，直接匹配
        if self.config.auto_link_by_username
            && let Some(username) = &user_info.username
        {
            let user = self
                .user_query
                .get_user_by_username(username)
                .await
                .map_err(|e| AuthError::Internal(e.to_string()))?;
            if let Some(user) = user {
                tracing::info!(provider = %provider, username = %username, user_id = %user.user_id, "username 匹配，自动关联");
                self.create_account(provider, provider_user_id, &user.user_id, user_info)
                    .await?;
                return Ok(LinkResult::Linked {
                    user_id: user.user_id,
                    is_new: false,
                });
            }
        }

        // 4. 自动注册策略
        if self.config.auto_register {
            let username = self.generate_username(provider, user_info).await?;
            let user_id = self
                .user_query
                .create_user_from_oauth2(
                    provider,
                    &OAuth2UserInfo {
                        provider: provider.to_string(),
                        provider_user_id: user_info.provider_user_id.clone(),
                        email: user_info.email.clone(),
                        username: Some(username),
                        display_name: user_info.display_name.clone(),
                        avatar_url: user_info.avatar_url.clone(),
                        default_role: self.config.default_role.clone(),
                    },
                )
                .await
                .map_err(|e| AuthError::Internal(e.to_string()))?;
            self.create_account(provider, provider_user_id, &user_id, user_info)
                .await?;
            tracing::info!(provider = %provider, provider_user_id = %provider_user_id, user_id = %user_id, "自动注册并关联");
            return Ok(LinkResult::Linked {
                user_id,
                is_new: true,
            });
        }

        // 5. 不自动注册，返回需要绑定
        tracing::info!(provider = %provider, provider_user_id = %provider_user_id, "需手动绑定");
        Ok(LinkResult::BindingRequired {
            provider: provider.to_string(),
            provider_user_id: provider_user_id.to_string(),
            email: user_info.email.clone(),
        })
    }

    /// 根据配置策略生成用户名（含冲突重试）。
    async fn generate_username(
        &self,
        provider: &str,
        user_info: &ProviderUserInfo,
    ) -> Result<String, AuthError> {
        const MAX_RETRIES: usize = 3;

        let base = match self.config.username_strategy.as_str() {
            "provider_prefix" => format!("{}_{}", provider, user_info.provider_user_id),
            "provider_user_id" => user_info.provider_user_id.clone(),
            "username" => user_info
                .username
                .clone()
                .unwrap_or_else(|| format!("{}_{}", provider, user_info.provider_user_id)),
            "email_prefix" => user_info
                .email
                .as_ref()
                .map(|e| e.split('@').next().unwrap_or(e).to_string())
                .unwrap_or_else(|| format!("{}_{}", provider, user_info.provider_user_id)),
            _ => user_info
                .display_name
                .clone()
                .unwrap_or_else(|| format!("{}_{}", provider, user_info.provider_user_id)),
        };

        if self
            .user_query
            .get_user_by_username(&base)
            .await
            .map_err(|e| AuthError::Internal(e.to_string()))?
            .is_none()
        {
            return Ok(base);
        }

        tracing::info!(base = %base, "用户名冲突，追加随机后缀重试");
        for i in 0..MAX_RETRIES {
            let suffix = Self::random_suffix();
            let candidate = format!("{}_{}", base, suffix);
            if self
                .user_query
                .get_user_by_username(&candidate)
                .await
                .map_err(|e| AuthError::Internal(e.to_string()))?
                .is_none()
            {
                tracing::info!(candidate = %candidate, attempt = i + 1, "用户名冲突重试成功");
                return Ok(candidate);
            }
        }

        tracing::warn!(base = %base, retries = MAX_RETRIES, "用户名冲突重试耗尽");
        Err(AuthError::OAuth2UsernameConflict(base))
    }

    /// 生成 4 位随机十六进制后缀。
    fn random_suffix() -> String {
        use std::fmt::Write;
        let mut buf = String::with_capacity(4);
        let val = rand::random::<u16>();
        write!(buf, "{:04x}", val).unwrap();
        buf
    }

    /// 解绑第三方账号（含安全检查）。
    ///
    /// 安全检查：当用户既没有设置密码也没有其他第三方绑定时，拒绝解绑，
    /// 防止用户失去所有登录方式。
    ///
    /// # Arguments
    ///
    /// * `user_id` - 本地用户 ID。
    /// * `provider` - 待解绑的 Provider 名称。
    ///
    /// # Errors
    ///
    /// * `AuthError::OAuth2LastBindingCannotRemove` - 解绑会导致用户无可用登录方式。
    /// * `AuthError::Internal` - 数据库查询或删除失败。
    pub async fn unlink_account(&self, user_id: &str, provider: &str) -> Result<(), AuthError> {
        // 1. 检查用户是否设置了密码
        let user = self
            .user_query
            .get_user_by_id(user_id)
            .await
            .map_err(|e| AuthError::Internal(e.to_string()))?;
        let has_password = user
            .as_ref()
            .and_then(|u| u.password_hash.as_ref())
            .is_some();

        // 2. 检查是否还绑定了其他第三方 Provider
        let other_bindings = self.count_other_bindings(user_id, provider).await?;

        // 3. 如果既没有密码也没有其他绑定，拒绝解绑
        if !has_password && other_bindings == 0 {
            tracing::warn!(user_id = %user_id, provider = %provider, "无法解除最后一个登录绑定");
            return Err(AuthError::OAuth2LastBindingCannotRemove);
        }

        // 4. 执行解绑
        self.remove_account(user_id, provider).await?;
        tracing::info!(user_id = %user_id, provider = %provider, "第三方账号解绑成功");
        Ok(())
    }
}

/// 从 `DataSet` 提取 `OAuth2Account`。
fn extract_oauth2_account(dataset: DataSet) -> Option<OAuth2Account> {
    let schema = dataset.schema.as_ref();
    let row = dataset.iter().next()?;

    Some(OAuth2Account {
        id: row.get_by_name_as(schema, "id").unwrap_or_default(),
        user_id: row.get_by_name_as(schema, "user_id").unwrap_or_default(),
        provider: row.get_by_name_as(schema, "provider").unwrap_or_default(),
        provider_user_id: row
            .get_by_name_as(schema, "provider_user_id")
            .unwrap_or_default(),
        provider_username: row.get_by_name_as(schema, "provider_username"),
        provider_email: row.get_by_name_as(schema, "provider_email"),
        provider_email_verified: row.get_by_name_as(schema, "provider_email_verified"),
        provider_display_name: row.get_by_name_as(schema, "provider_display_name"),
        provider_avatar_url: row.get_by_name_as(schema, "provider_avatar_url"),
    })
}
