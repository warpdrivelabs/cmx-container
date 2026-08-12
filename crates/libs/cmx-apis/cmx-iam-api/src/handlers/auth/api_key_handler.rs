//! API Key 管理 Handler
//!
//! 提供 API Key 的创建/列表/删除/启用禁用等管理 API。
//! API Key 明文仅在创建时返回一次，后续不可查看。

use axum::Json;
use axum::extract::{Query, State};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use cmx_api_core::CmxAppState;
use cmx_api_core::middleware::CmxSvrContext;
use cmx_api_core::{ApiResp, Error, Result};

use cmx_iam::api_key::store;

/// 生成随机 API Key 明文
fn generate_api_key() -> String {
    format!("cmx_{}", uuid::Uuid::new_v4().simple())
}

/// SHA256 哈希
fn sha256(input: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    input.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// 失效 API Key 的 Redis 两层缓存。
///
/// API Key 缓存为 Redis-only（无本地 moka L1），直接删除 Redis key 即可对所有实例生效。
async fn invalidate_api_key_cache(key_prefix: &str) {
    if let Some(cache) = cmx_buffer::GlobalCacheManager::try_get() {
        let entity_key = format!("auth:api_key:{}", key_prefix);
        let ctx_key = format!("auth:api_key_ctx:{}", key_prefix);
        let keys = [entity_key.as_str(), ctx_key.as_str()];
        match cache.ops().del_batch(&keys).await {
            Ok(_) => debug!(key_prefix = %key_prefix, "API Key 缓存已失效"),
            Err(e) => warn!(key_prefix = %key_prefix, error = %e, "API Key 缓存失效失败"),
        }
    }
}

/// 创建 API Key 请求
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct CreateApiKeyRequest {
    /// 关联用户 ID（纯服务间调用时为空）
    #[serde(default)]
    pub user_id: Option<String>,
    /// 关联服务名称（如 billing-service）
    #[serde(default)]
    pub service_name: Option<String>,
    /// 允许的 scope 列表
    #[serde(default)]
    pub scopes: Vec<String>,
    /// 描述/备注
    #[serde(default)]
    pub description: Option<String>,
}

/// API Key 响应（含明文，仅创建时返回）
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ApiKeyResponse {
    pub id: String,
    pub key_prefix: String,
    /// API Key 明文（仅创建时返回，后续不可查看）
    pub api_key: String,
    pub user_id: Option<String>,
    pub service_name: Option<String>,
    pub scopes: Vec<String>,
    pub description: Option<String>,
    pub status: i64,
    pub create_time: String,
}

/// API Key 列表项（不含明文）
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ApiKeyListItem {
    pub id: String,
    pub key_prefix: String,
    pub user_id: Option<String>,
    pub service_name: Option<String>,
    pub scopes: Vec<String>,
    pub description: Option<String>,
    pub status: i64,
    pub create_time: String,
}

/// API Key 查询参数
#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct ApiKeyQuery {
    /// 按状态过滤：1-启用，0-禁用，不传=全部
    pub status: Option<i64>,
    /// 按 user_id 过滤
    pub user_id: Option<String>,
    /// 按 service_name 过滤
    pub service_name: Option<String>,
}

/// 启用/禁用请求
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct ToggleApiKeyStatusRequest {
    pub id: String,
    pub status: i64, // 0-禁用，1-启用
}

/// 创建 API Key
#[utoipa::path(
    post,
    path = "/api/auth/api-keys/create",
    request_body = CreateApiKeyRequest,
    responses(
        (status = 200, description = "创建成功，api_key 字段为明文（仅此一次）", body = ApiResp<ApiKeyResponse>)
    ),
    tag = "Auth-ApiKey"
)]
pub async fn create_api_key(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Json(req): Json<CreateApiKeyRequest>,
) -> Result<Json<ApiResp<ApiKeyResponse>>> {
    debug!(
        "{:<12} - handler::create_api_key - service: {:?}",
        "HANDLER", req.service_name
    );

    let raw_key = generate_api_key();
    let key_prefix = raw_key.len().min(8);
    let key_prefix_str = &raw_key[..key_prefix];
    let key_hash = sha256(&raw_key);
    let id = cmx_utils::snowflake_id_str();
    let scopes_json = serde_json::to_string(&req.scopes).unwrap_or_else(|_| "[]".to_string());

    store::insert_api_key(
        &id,
        key_prefix_str,
        &key_hash,
        req.user_id.clone(),
        req.service_name.clone(),
        scopes_json,
        req.description.clone(),
    )
    .await
    .map_err(|e| Error::InternalError(format!("创建 API Key 失败: {e}")))?;

    info!(key_prefix = key_prefix_str, "API Key 创建成功");

    Ok(Json(ApiResp::ok(ApiKeyResponse {
        id,
        key_prefix: key_prefix_str.to_string(),
        api_key: raw_key,
        user_id: req.user_id,
        service_name: req.service_name,
        scopes: req.scopes,
        description: req.description,
        status: 1,
        create_time: chrono::Utc::now().to_rfc3339(),
    })))
}

/// 查询 API Key 列表
#[utoipa::path(
    get,
    path = "/api/auth/api-keys/list",
    params(
        ApiKeyQuery
    ),
    responses(
        (status = 200, description = "查询成功", body = ApiResp<Vec<ApiKeyListItem>>)
    ),
    tag = "Auth-ApiKey"
)]
pub async fn list_api_keys(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Query(params): Query<ApiKeyQuery>,
) -> Result<Json<ApiResp<Vec<ApiKeyListItem>>>> {
    debug!("{:<12} - handler::list_api_keys", "HANDLER");

    let dataset = store::list_api_keys(params.status, params.user_id, params.service_name)
        .await
        .map_err(|e| Error::InternalError(format!("查询 API Key 列表失败: {e}")))?;

    let schema = dataset.schema.as_ref();
    let items: Vec<ApiKeyListItem> = dataset
        .iter()
        .filter_map(|row| {
            let scopes_str: String = row.get_by_name_as(schema, "scopes").unwrap_or_default();
            let scopes: Vec<String> = serde_json::from_str(&scopes_str).unwrap_or_default();
            Some(ApiKeyListItem {
                id: row.get_by_name_as(schema, "id")?,
                key_prefix: row.get_by_name_as(schema, "key_prefix")?,
                user_id: row.get_by_name_as(schema, "user_id"),
                service_name: row.get_by_name_as(schema, "service_name"),
                scopes,
                description: row.get_by_name_as(schema, "description"),
                status: row.get_by_name_as::<i64>(schema, "status").unwrap_or(1),
                create_time: row
                    .get_by_name_as::<String>(schema, "create_time")
                    .unwrap_or_default(),
            })
        })
        .collect();

    Ok(Json(ApiResp::ok(items)))
}

/// 删除 API Key
#[utoipa::path(
    post,
    path = "/api/auth/api-keys/delete",
    responses(
        (status = 200, description = "删除成功", body = ApiResp<serde_json::Value>)
    ),
    tag = "Auth-ApiKey"
)]
pub async fn delete_api_key(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Json(req): Json<serde_json::Value>,
) -> Result<Json<ApiResp<()>>> {
    let id = req
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::BusinessError("缺少 id 参数".to_string()))?;

    debug!("{:<12} - handler::delete_api_key - id: {}", "HANDLER", id);

    // 删除前查询 key_prefix，用于删除后失效 Redis 缓存
    let key_prefix = store::query_key_prefix_by_id(id).await;

    let affected = store::delete_api_key(id)
        .await
        .map_err(|e| Error::InternalError(format!("删除 API Key 失败: {e}")))?;

    if affected == 0 {
        return Err(Error::BusinessError(format!("API Key 不存在: {id}")));
    }

    // 失效 Redis 两层缓存（ApiKeyEntity + AuthContext）
    if let Some(prefix) = key_prefix {
        invalidate_api_key_cache(&prefix).await;
    }

    info!(api_key_id = id, "API Key 已删除");
    Ok(Json(ApiResp::ok(())))
}

/// 启用/禁用 API Key
#[utoipa::path(
    post,
    path = "/api/auth/api-keys/toggle-status",
    request_body = ToggleApiKeyStatusRequest,
    responses(
        (status = 200, description = "切换成功", body = ApiResp<serde_json::Value>)
    ),
    tag = "Auth-ApiKey"
)]
pub async fn toggle_api_key_status(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Json(req): Json<ToggleApiKeyStatusRequest>,
) -> Result<Json<ApiResp<()>>> {
    debug!(
        "{:<12} - handler::toggle_api_key_status - id: {}, status: {}",
        "HANDLER", req.id, req.status
    );

    // 查询 key_prefix，用于状态切换后失效 Redis 缓存
    let key_prefix = store::query_key_prefix_by_id(&req.id).await;

    let affected = store::set_api_key_status(&req.id, req.status)
        .await
        .map_err(|e| Error::InternalError(format!("切换 API Key 状态失败: {e}")))?;

    if affected == 0 {
        warn!("API Key 不存在或已归档: {}", req.id);
        return Err(Error::BusinessError(format!(
            "API Key 不存在或已归档: {}",
            req.id
        )));
    }

    // 失效 Redis 两层缓存（ApiKeyEntity + AuthContext）
    if let Some(prefix) = key_prefix {
        invalidate_api_key_cache(&prefix).await;
    }

    Ok(Json(ApiResp::ok(())))
}
