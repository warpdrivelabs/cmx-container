//! Auth 表存储查询 trait — 从 UserAuthQuery 拆分。
//!
//! 操作 `cmx_auth_*` 表，由 cmx-auth 自行实现。

use async_trait::async_trait;

use crate::error::TraitError;
use super::user_query::{ApiKeyData, OAuth2ClientData};

/// Auth 表存储查询 — 由 cmx-auth 自行实现。
///
/// 从 UserAuthQuery 拆分出的 Auth 专属方法，操作 `cmx_auth_*` 表。
#[async_trait]
pub trait AuthStorageQuery: Send + Sync {
    /// 新增或更新 API Key。
    ///
    /// # Arguments
    ///
    /// * `key_prefix` - API Key 前缀（唯一标识）。
    /// * `key_hash` - API Key 的 SHA256 哈希。
    /// * `user_id` - 关联用户 ID（服务级 Key 为 `None`）。
    /// * `service_name` - 关联服务名称（用户级 Key 为 `None`）。
    /// * `scopes` - 允许的 scope 列表。
    /// * `description` - 描述信息。
    ///
    /// # Errors
    ///
    /// 写入失败时返回 [`TraitError`]。
    async fn upsert_api_key(
        &self,
        key_prefix: &str,
        key_hash: &str,
        user_id: Option<&str>,
        service_name: Option<&str>,
        scopes: &[String],
        description: Option<&str>,
    ) -> Result<(), TraitError>;

    /// 按 prefix 查询 API Key。
    ///
    /// # Arguments
    ///
    /// * `key_prefix` - API Key 前缀。
    ///
    /// # Returns
    ///
    /// 存在时返回 `Ok(Some(key))`，不存在时返回 `Ok(None)`。
    ///
    /// # Errors
    ///
    /// 查询失败时返回 [`TraitError`]。
    async fn get_api_key_by_prefix(
        &self,
        key_prefix: &str,
    ) -> Result<Option<ApiKeyData>, TraitError>;

    /// 记录 Token 事件。
    ///
    /// # Arguments
    ///
    /// * `event_type` - 事件类型（如 `issued`、`revoked`、`expired`）。
    /// * `user_id` - 用户 ID。
    /// * `jti` - Token 唯一标识。
    /// * `detail` - 事件详情。
    ///
    /// # Errors
    ///
    /// 写入失败时返回 [`TraitError`]。
    async fn record_token_event(
        &self,
        event_type: &str,
        user_id: &str,
        jti: &str,
        detail: &str,
    ) -> Result<(), TraitError>;

    /// 查询 OAuth2 客户端配置。
    ///
    /// # Arguments
    ///
    /// * `client_id` - 客户端 ID。
    ///
    /// # Returns
    ///
    /// 客户端存在时返回 `Ok(Some(client))`，不存在时返回 `Ok(None)`。
    ///
    /// # Errors
    ///
    /// 查询失败时返回 [`TraitError`]。
    async fn get_oauth2_client(
        &self,
        client_id: &str,
    ) -> Result<Option<OAuth2ClientData>, TraitError>;
}
