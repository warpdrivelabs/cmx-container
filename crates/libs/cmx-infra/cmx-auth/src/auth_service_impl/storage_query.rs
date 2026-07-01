//! 认证数据持久化查询（AuthStorageQuery 实现）
//!
//! 实现 [`cmx_traits::auth::AuthStorageQuery`] 的 API Key upsert/查询、
//! Token 事件记录、OAuth2 客户端查询。这些方法直接操作 `cmx_auth_*` 表，
//! trait 实现在 `mod.rs` 中逐方法委托。

use cmx_core::model::cell::DataValue;
use cmx_traits::auth::{ApiKeyData, OAuth2ClientData};
use cmx_traits::error::TraitError;
use cmx_utils::snowflake_id_str;
use tracing::{debug, info};

use crate::auth_service_impl::AuthServiceImpl;

impl AuthServiceImpl {
    /// 新增或更新 API Key 记录。
    ///
    /// 以 `key_prefix` 为唯一键执行 `INSERT ... ON CONFLICT DO UPDATE`，
    /// 已存在时覆盖 `key_hash`/`user_id`/`service_name`/`scopes`/`description`。
    ///
    /// # Arguments
    ///
    /// * `key_prefix` - API Key 前缀（唯一标识）。
    /// * `key_hash` - Key 的 SHA256 哈希。
    /// * `user_id` - 关联用户 ID（可选，纯服务间调用时为 `None`）。
    /// * `service_name` - 关联服务名称（可选）。
    /// * `scopes` - 允许的 scope 列表。
    /// * `description` - 描述/备注（可选）。
    ///
    /// # Errors
    ///
    /// 当 SQL 执行失败或 scopes 序列化失败时返回 `TraitError::Internal`。
    pub(super) async fn upsert_api_key(
        &self,
        key_prefix: &str,
        key_hash: &str,
        user_id: Option<&str>,
        service_name: Option<&str>,
        scopes: &[String],
        description: Option<&str>,
    ) -> std::result::Result<(), TraitError> {
        debug!(
            "{:<12} - AuthServiceImpl::upsert_api_key - key_prefix: {}",
            "AUTH", key_prefix
        );

        let id = snowflake_id_str();
        let scopes_json = serde_json::to_string(scopes)
            .map_err(|e| TraitError::Internal(format!("序列化 scopes 失败: {}", e)))?;

        // 参数化查询：使用 $1..$7 占位符，由数据库驱动处理转义，避免 SQL 注入风险
        let sql = "INSERT INTO cmx_auth_api_key (id, key_prefix, key_hash, user_id, service_name, scopes, description, archived, status) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, 0, 1) \
             ON CONFLICT (key_prefix) DO UPDATE SET key_hash = EXCLUDED.key_hash, user_id = EXCLUDED.user_id, \
             service_name = EXCLUDED.service_name, scopes = EXCLUDED.scopes, description = EXCLUDED.description";

        let params: Vec<DataValue> = vec![
            DataValue::String(id),
            DataValue::String(key_prefix.to_string()),
            DataValue::String(key_hash.to_string()),
            user_id.map(|u| DataValue::String(u.to_string())).unwrap_or(DataValue::Null),
            service_name.map(|s| DataValue::String(s.to_string())).unwrap_or(DataValue::Null),
            DataValue::String(scopes_json),
            description.map(|d| DataValue::String(d.to_string())).unwrap_or(DataValue::Null),
        ];

        let db_manager = cmx_database::get_default_db_manager();
        let db_id = db_manager.get_default_db_id().await;
        db_manager
            .execute_sql_with_datavalues(&db_id, None, sql, params)
            .await
            .map_err(|e| TraitError::Internal(format!("导入 API Key 失败: {}", e)))?;

        info!(key_prefix = key_prefix, "静态 API Key 已导入");
        Ok(())
    }

    /// 根据 `key_prefix` 查询未归档的 API Key 记录。
    ///
    /// # Arguments
    ///
    /// * `key_prefix` - API Key 前缀（前 8 位）。
    ///
    /// # Returns
    ///
    /// 存在时返回 `Some(ApiKeyData)`，否则返回 `None`。
    ///
    /// # Errors
    ///
    /// 当数据库查询失败时返回 `TraitError::Internal`。
    pub(super) async fn get_api_key_by_prefix(
        &self,
        key_prefix: &str,
    ) -> std::result::Result<Option<ApiKeyData>, TraitError> {
        debug!(
            "{:<12} - AuthServiceImpl::get_api_key_by_prefix - key_prefix: {}",
            "AUTH", key_prefix
        );

        // 参数化查询：使用 $1 占位符，由数据库驱动处理转义
        let sql = "SELECT key_prefix, key_hash, user_id, service_name, scopes, description, status \
             FROM cmx_auth_api_key WHERE key_prefix = $1 AND archived = 0";

        let params: Vec<DataValue> = vec![DataValue::String(key_prefix.to_string())];

        let db_manager = cmx_database::get_default_db_manager();
        let db_id = db_manager.get_default_db_id().await;
        let dataset = db_manager
            .query_sql_with_datavalues(&db_id, None, sql, params, "api_key_by_prefix")
            .await
            .map_err(|e| TraitError::Internal(format!("查询 API Key 失败: {}", e)))?;

        let schema = dataset.schema.as_ref();
        let row = match dataset.iter().next() {
            Some(r) => r,
            None => return Ok(None),
        };

        let scopes_str: String = row.get_by_name_as(schema, "scopes").unwrap_or_default();
        let scopes: Vec<String> = serde_json::from_str(&scopes_str).unwrap_or_default();

        Ok(Some(ApiKeyData {
            key_prefix: row.get_by_name_as(schema, "key_prefix").unwrap_or_default(),
            key_hash: row.get_by_name_as(schema, "key_hash").unwrap_or_default(),
            user_id: row.get_by_name_as(schema, "user_id"),
            service_name: row.get_by_name_as(schema, "service_name"),
            scopes,
            description: row.get_by_name_as(schema, "description"),
            status: row.get_by_name_as::<i64>(schema, "status").unwrap_or(1),
        }))
    }

    /// 记录 Token 生命周期事件到审计表。
    ///
    /// 用于 Token 签发、撤销、密码修改等关键事件的持久化审计。
    ///
    /// # Arguments
    ///
    /// * `event_type` - 事件类型（如 `token_issued`/`token_revoked`/`password_changed`）。
    /// * `user_id` - 关联用户 ID。
    /// * `jti` - 关联 Token 的 JTI（无关联时传空字符串）。
    /// * `detail` - 事件详情描述。
    ///
    /// # Errors
    ///
    /// 当 SQL 执行失败时返回 `TraitError::Internal`。
    pub(super) async fn record_token_event(
        &self,
        event_type: &str,
        user_id: &str,
        jti: &str,
        detail: &str,
    ) -> std::result::Result<(), TraitError> {
        debug!(
            "{:<12} - AuthServiceImpl::record_token_event - event: {}, user: {}",
            "AUTH", event_type, user_id
        );

        let id = snowflake_id_str();
        // 参数化查询：使用 $1..$5 占位符，由数据库驱动处理转义
        let sql = "INSERT INTO cmx_auth_token_event (id, event_type, user_id, jti, detail, create_time) \
             VALUES ($1, $2, $3, $4, $5, NOW())";

        let params: Vec<DataValue> = vec![
            DataValue::String(id),
            DataValue::String(event_type.to_string()),
            DataValue::String(user_id.to_string()),
            DataValue::String(jti.to_string()),
            DataValue::String(detail.to_string()),
        ];

        let db_manager = cmx_database::get_default_db_manager();
        let db_id = db_manager.get_default_db_id().await;
        db_manager
            .execute_sql_with_datavalues(&db_id, None, sql, params)
            .await
            .map_err(|e| TraitError::Internal(format!("记录 Token 事件失败: {}", e)))?;

        Ok(())
    }

    /// 根据 `client_id` 查询未归档的 OAuth2 客户端信息。
    ///
    /// # Arguments
    ///
    /// * `client_id` - OAuth2 客户端 ID。
    ///
    /// # Returns
    ///
    /// 客户端存在时返回 `Some(OAuth2ClientData)`，否则返回 `None`。
    ///
    /// # Errors
    ///
    /// 当数据库查询失败时返回 `TraitError::Internal`。
    pub(super) async fn get_oauth2_client(
        &self,
        client_id: &str,
    ) -> std::result::Result<Option<OAuth2ClientData>, TraitError> {
        debug!(
            "{:<12} - AuthServiceImpl::get_oauth2_client - client_id: {}",
            "AUTH", client_id
        );

        // 参数化查询：使用 $1 占位符，由数据库驱动处理转义
        let sql = "SELECT client_id, client_name, client_secret, redirect_uris, grant_types, \
             client_type, pkce_required, allowed_scopes, status \
             FROM cmx_auth_client WHERE client_id = $1 AND archived = 0";

        let params: Vec<DataValue> = vec![DataValue::String(client_id.to_string())];

        let db_manager = cmx_database::get_default_db_manager();
        let db_id = db_manager.get_default_db_id().await;
        let dataset = db_manager
            .query_sql_with_datavalues(&db_id, None, sql, params, "oauth2_client")
            .await
            .map_err(|e| TraitError::Internal(format!("查询 OAuth2 客户端失败: {}", e)))?;

        let schema = dataset.schema.as_ref();
        let row = match dataset.iter().next() {
            Some(r) => r,
            None => return Ok(None),
        };

        // 解析 JSON 字段为 Vec<String>
        let redirect_uris_str: String =
            row.get_by_name_as(schema, "redirect_uris").unwrap_or_default();
        let redirect_uris: Vec<String> = serde_json::from_str(&redirect_uris_str).unwrap_or_default();

        let grant_types_str: String =
            row.get_by_name_as(schema, "grant_types").unwrap_or_default();
        let grant_types: Vec<String> = serde_json::from_str(&grant_types_str).unwrap_or_default();

        let allowed_scopes_str: String =
            row.get_by_name_as(schema, "allowed_scopes").unwrap_or_default();
        let allowed_scopes: Vec<String> = serde_json::from_str(&allowed_scopes_str).unwrap_or_default();

        let pkce_required: bool = row
            .get_by_name_as::<i64>(schema, "pkce_required")
            .map(|v| v != 0)
            .unwrap_or(true);

        Ok(Some(OAuth2ClientData {
            client_id: row.get_by_name_as(schema, "client_id").unwrap_or_default(),
            client_name: row.get_by_name_as(schema, "client_name").unwrap_or_default(),
            client_secret: row.get_by_name_as(schema, "client_secret"),
            redirect_uris,
            grant_types,
            client_type: row
                .get_by_name_as(schema, "client_type")
                .unwrap_or_else(|| "public".to_string()),
            pkce_required,
            allowed_scopes,
            status: row.get_by_name_as::<i64>(schema, "status").unwrap_or(1),
        }))
    }
}
