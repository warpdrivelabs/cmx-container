//! Auth 表存储查询 trait — 从 UserAuthQuery 拆分
//!
//! 操作 cmx_auth_* 表，由 cmx-auth 自行实现。

use async_trait::async_trait;

use crate::error::TraitError;
use super::user_query::{ApiKeyData, OAuth2ClientData};

/// Auth 表存储查询 — 由 cmx-auth 自行实现
///
/// 从 UserAuthQuery 拆分出的 Auth 专属方法，操作 cmx_auth_* 表
#[async_trait]
pub trait AuthStorageQuery: Send + Sync {
    /// 新增或更新 API Key
    async fn upsert_api_key(
        &self,
        key_prefix: &str,
        key_hash: &str,
        user_id: Option<&str>,
        service_name: Option<&str>,
        scopes: &[String],
        description: Option<&str>,
    ) -> Result<(), TraitError>;

    /// 按 prefix 查询 API Key
    async fn get_api_key_by_prefix(
        &self,
        key_prefix: &str,
    ) -> Result<Option<ApiKeyData>, TraitError>;

    /// 记录 Token 事件
    async fn record_token_event(
        &self,
        event_type: &str,
        user_id: &str,
        jti: &str,
        detail: &str,
    ) -> Result<(), TraitError>;

    /// 查询 OAuth2 客户端配置
    async fn get_oauth2_client(
        &self,
        client_id: &str,
    ) -> Result<Option<OAuth2ClientData>, TraitError>;
}
