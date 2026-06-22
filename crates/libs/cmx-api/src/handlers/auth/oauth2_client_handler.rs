//! OAuth2 客户端管理 Handler
//!
//! 提供 OAuth2 客户端的 CRUD 管理接口。
//! client_secret 在创建时哈希存储，更新时支持重置。

use axum::extract::{Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use crate::app_state::CmxAppState;
use crate::middleware::CmxSvrContext;
use crate::{ApiResp, Error, Result};

/// 哈希密钥（简化版，使用与 API Key 相同的哈希算法）
fn hash_secret(secret: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    secret.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// 创建 OAuth2 客户端请求
#[derive(Debug, Deserialize, Serialize)]
#[derive(utoipa::ToSchema)]
pub struct CreateOAuth2ClientRequest {
    /// 客户端标识（唯一）
    pub client_id: String,
    /// 客户端名称
    pub client_name: String,
    /// 客户端密钥明文（confidential 类型必填，public 类型可空）
    #[serde(default)]
    pub client_secret: Option<String>,
    /// 客户端类型：public / confidential
    pub client_type: String,
    /// 回调地址列表
    pub redirect_uris: Vec<String>,
    /// 允许的授权类型（逗号分隔，如 authorization_code,refresh_token）
    pub grant_types: Vec<String>,
    /// 允许的 scope（逗号分隔）
    #[serde(default)]
    pub allowed_scopes: Vec<String>,
    /// 是否强制 PKCE
    #[serde(default = "default_pkce")]
    pub pkce_required: bool,
    /// 描述
    #[serde(default)]
    pub description: Option<String>,
}

fn default_pkce() -> bool {
    true
}

/// 更新 OAuth2 客户端请求
#[derive(Debug, Deserialize, Serialize)]
#[derive(utoipa::ToSchema)]
pub struct UpdateOAuth2ClientRequest {
    /// 客户端名称
    #[serde(default)]
    pub client_name: Option<String>,
    /// 新密钥明文（传则重置密钥，不传不修改）
    #[serde(default)]
    pub client_secret: Option<String>,
    /// 回调地址列表
    #[serde(default)]
    pub redirect_uris: Option<Vec<String>>,
    /// 允许的授权类型
    #[serde(default)]
    pub grant_types: Option<Vec<String>>,
    /// 允许的 scope
    #[serde(default)]
    pub allowed_scopes: Option<Vec<String>>,
    /// 是否强制 PKCE
    #[serde(default)]
    pub pkce_required: Option<bool>,
    /// 描述
    #[serde(default)]
    pub description: Option<String>,
    /// 状态
    #[serde(default)]
    pub status: Option<i64>,
}

/// OAuth2 客户端响应
#[derive(Debug, Serialize)]
#[derive(utoipa::ToSchema)]
pub struct OAuth2ClientResponse {
    pub id: String,
    pub client_id: String,
    pub client_name: String,
    pub client_type: String,
    pub redirect_uris: Vec<String>,
    pub grant_types: Vec<String>,
    pub allowed_scopes: Vec<String>,
    pub pkce_required: bool,
    pub status: i64,
    pub description: Option<String>,
    pub create_time: String,
    pub update_time: String,
}

/// OAuth2 客户端查询参数
#[derive(Debug, Deserialize)]
#[derive(utoipa::IntoParams)]
pub struct OAuth2ClientQuery {
    /// 按状态过滤
    pub status: Option<i64>,
    /// 按 client_id 过滤
    pub client_id: Option<String>,
}

/// 创建 OAuth2 客户端
#[utoipa::path(
    post,
    path = "/api/auth/oauth2-clients/create",
    request_body = CreateOAuth2ClientRequest,
    responses(
        (status = 200, description = "创建成功")
    ),
    tag = "Auth-OAuth2Client"
)]
pub async fn create_oauth2_client(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Json(req): Json<CreateOAuth2ClientRequest>,
) -> Result<Json<ApiResp<OAuth2ClientResponse>>> {
    debug!(
        "{:<12} - handler::create_oauth2_client - client_id: {}",
        "HANDLER", req.client_id
    );

    let id = cmx_utils::snowflake_id_str();
    let redirect_uris_json = serde_json::to_string(&req.redirect_uris).unwrap_or_else(|_| "[]".to_string());
    let grant_types_str = req.grant_types.join(",");
    let allowed_scopes_str = req.allowed_scopes.join(",");

    // confidential 类型必须有 secret
    let secret_hash = if req.client_type == "confidential" {
        let secret = req
            .client_secret
            .as_deref()
            .ok_or_else(|| Error::BusinessError("confidential 客户端必须提供 client_secret".to_string()))?;
        if secret.len() < 8 {
            return Err(Error::BusinessError(
                "client_secret 长度不能少于 8 位".to_string(),
            ));
        }
        Some(hash_secret(secret))
    } else {
        None
    };

    let db_manager = cmx_database::get_default_db_manager();
    let db_id = db_manager.get_default_db_id().await;

    let sql = r#"
        INSERT INTO cmx_auth_client (id, client_id, client_name, client_secret, client_type,
            redirect_uris, grant_types, allowed_scopes, pkce_required, description, status, archived)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, 1, 0)
    "#;
    let params = serde_json::Value::Array(vec![
        serde_json::Value::String(id.clone()),
        serde_json::Value::String(req.client_id.clone()),
        serde_json::Value::String(req.client_name.clone()),
        secret_hash
            .map(serde_json::Value::String)
            .unwrap_or(serde_json::Value::Null),
        serde_json::Value::String(req.client_type.clone()),
        serde_json::Value::String(redirect_uris_json),
        serde_json::Value::String(grant_types_str),
        serde_json::Value::String(allowed_scopes_str),
        serde_json::Value::Bool(req.pkce_required),
        req.description
            .clone()
            .map(serde_json::Value::String)
            .unwrap_or(serde_json::Value::Null),
    ]);
    db_manager
        .execute_sql_with_json(&db_id, None, sql, params)
        .await
        .map_err(|e| {
            let msg = format!("{e}");
            if msg.contains("duplicate") || msg.contains("unique") {
                Error::BusinessError(format!("client_id 已存在: {}", req.client_id))
            } else {
                Error::InternalError(format!("创建 OAuth2 客户端失败: {e}"))
            }
        })?;

    info!(client_id = req.client_id, "OAuth2 客户端创建成功");

    Ok(Json(ApiResp::ok(OAuth2ClientResponse {
        id,
        client_id: req.client_id,
        client_name: req.client_name,
        client_type: req.client_type,
        redirect_uris: req.redirect_uris,
        grant_types: req.grant_types,
        allowed_scopes: req.allowed_scopes,
        pkce_required: req.pkce_required,
        status: 1,
        description: req.description,
        create_time: chrono::Utc::now().to_rfc3339(),
        update_time: chrono::Utc::now().to_rfc3339(),
    })))
}

/// 查询 OAuth2 客户端列表
#[utoipa::path(
    get,
    path = "/api/auth/oauth2-clients/list",
    params(
        OAuth2ClientQuery
    ),
    responses(
        (status = 200, description = "查询成功")
    ),
    tag = "Auth-OAuth2Client"
)]
pub async fn list_oauth2_clients(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Query(params): Query<OAuth2ClientQuery>,
) -> Result<Json<ApiResp<Vec<OAuth2ClientResponse>>>> {
    debug!("{:<12} - handler::list_oauth2_clients", "HANDLER");

    let db_manager = cmx_database::get_default_db_manager();
    let db_id = db_manager.get_default_db_id().await;

    let mut where_clause = String::from("archived = 0");
    if let Some(status) = params.status {
        where_clause.push_str(&format!(" AND status = {}", status));
    }
    if let Some(cid) = &params.client_id {
        where_clause.push_str(&format!(" AND client_id = '{}'", cid.replace('\'', "''")));
    }

    let sql = format!(
        "SELECT id, client_id, client_name, client_type, redirect_uris, grant_types, \
         allowed_scopes, pkce_required, status, description, create_time, update_time \
         FROM cmx_auth_client WHERE {where_clause} ORDER BY create_time DESC"
    );

    let dataset = db_manager
        .query_sql(&db_id, None, &sql, "oauth2_clients_list")
        .await
        .map_err(|e| Error::InternalError(format!("查询 OAuth2 客户端列表失败: {e}")))?;

    let schema = dataset.schema.as_ref();
    let items: Vec<OAuth2ClientResponse> = dataset
        .iter()
        .filter_map(|row| {
            let redirect_uris_str: String =
                row.get_by_name_as(schema, "redirect_uris").unwrap_or_default();
            let redirect_uris: Vec<String> =
                serde_json::from_str(&redirect_uris_str).unwrap_or_default();
            let grant_types_str: String =
                row.get_by_name_as(schema, "grant_types").unwrap_or_default();
            let grant_types: Vec<String> = grant_types_str
                .split(',')
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect();
            let allowed_scopes_str: String =
                row.get_by_name_as(schema, "allowed_scopes").unwrap_or_default();
            let allowed_scopes: Vec<String> = allowed_scopes_str
                .split(',')
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect();
            Some(OAuth2ClientResponse {
                id: row.get_by_name_as(schema, "id")?,
                client_id: row.get_by_name_as(schema, "client_id")?,
                client_name: row.get_by_name_as(schema, "client_name")?,
                client_type: row.get_by_name_as(schema, "client_type")?,
                redirect_uris,
                grant_types,
                allowed_scopes,
                pkce_required: row
                    .get_by_name_as::<bool>(schema, "pkce_required")
                    .unwrap_or(true),
                status: row.get_by_name_as::<i64>(schema, "status").unwrap_or(1),
                description: row.get_by_name_as(schema, "description"),
                create_time: row
                    .get_by_name_as::<String>(schema, "create_time")
                    .unwrap_or_default(),
                update_time: row
                    .get_by_name_as::<String>(schema, "update_time")
                    .unwrap_or_default(),
            })
        })
        .collect();

    Ok(Json(ApiResp::ok(items)))
}

/// 更新 OAuth2 客户端
#[utoipa::path(
    post,
    path = "/api/auth/oauth2-clients/update",
    request_body = UpdateOAuth2ClientRequest,
    responses(
        (status = 200, description = "更新成功")
    ),
    tag = "Auth-OAuth2Client"
)]
pub async fn update_oauth2_client(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Json(req): Json<UpdateOAuth2ClientRequest>,
) -> Result<Json<ApiResp<()>>> {
    debug!("{:<12} - handler::update_oauth2_client", "HANDLER");

    // 需要提供 client_id 来定位记录
    let client_id = req
        .client_name
        .as_ref()
        .map(|_| ())
        .and_then(|_| None::<&str>);
    let _ = client_id; // placeholder

    // 使用 client_id 字段定位（从请求中获取，需要额外字段）
    // 这里简化：通过 client_id 查询后更新
    Err(Error::BusinessError(
        "请使用 /api/auth/oauth2-clients/update-by-id 并提供 client_id".to_string(),
    ))
}

/// 更新 OAuth2 客户端（按 client_id）
#[derive(Debug, Deserialize, Serialize)]
#[derive(utoipa::ToSchema)]
pub struct UpdateOAuth2ClientByIdRequest {
    /// 客户端标识（定位用）
    pub client_id: String,
    /// 客户端名称
    #[serde(default)]
    pub client_name: Option<String>,
    /// 新密钥明文（传则重置密钥）
    #[serde(default)]
    pub client_secret: Option<String>,
    /// 回调地址列表
    #[serde(default)]
    pub redirect_uris: Option<Vec<String>>,
    /// 允许的授权类型
    #[serde(default)]
    pub grant_types: Option<Vec<String>>,
    /// 允许的 scope
    #[serde(default)]
    pub allowed_scopes: Option<Vec<String>>,
    /// 是否强制 PKCE
    #[serde(default)]
    pub pkce_required: Option<bool>,
    /// 描述
    #[serde(default)]
    pub description: Option<String>,
    /// 状态
    #[serde(default)]
    pub status: Option<i64>,
}

/// 更新 OAuth2 客户端（按 client_id）
#[utoipa::path(
    post,
    path = "/api/auth/oauth2-clients/update",
    request_body = UpdateOAuth2ClientByIdRequest,
    responses(
        (status = 200, description = "更新成功")
    ),
    tag = "Auth-OAuth2Client"
)]
pub async fn update_oauth2_client_by_id(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Json(req): Json<UpdateOAuth2ClientByIdRequest>,
) -> Result<Json<ApiResp<()>>> {
    debug!(
        "{:<12} - handler::update_oauth2_client_by_id - client_id: {}",
        "HANDLER", req.client_id
    );

    let db_manager = cmx_database::get_default_db_manager();
    let db_id = db_manager.get_default_db_id().await;

    let mut sets: Vec<String> = Vec::new();
    let mut params: Vec<serde_json::Value> = vec![serde_json::Value::String(req.client_id.clone())];
    let mut idx = 2;

    if let Some(name) = &req.client_name {
        sets.push(format!("client_name = ${idx}"));
        params.push(serde_json::Value::String(name.clone()));
        idx += 1;
    }
    if let Some(secret) = &req.client_secret {
        if secret.len() < 8 {
            return Err(Error::BusinessError(
                "client_secret 长度不能少于 8 位".to_string(),
            ));
        }
        sets.push(format!("client_secret = ${idx}"));
        params.push(serde_json::Value::String(hash_secret(secret)));
        idx += 1;
    }
    if let Some(uris) = &req.redirect_uris {
        let json = serde_json::to_string(uris).unwrap_or_else(|_| "[]".to_string());
        sets.push(format!("redirect_uris = ${idx}"));
        params.push(serde_json::Value::String(json));
        idx += 1;
    }
    if let Some(gt) = &req.grant_types {
        sets.push(format!("grant_types = ${idx}"));
        params.push(serde_json::Value::String(gt.join(",")));
        idx += 1;
    }
    if let Some(scopes) = &req.allowed_scopes {
        sets.push(format!("allowed_scopes = ${idx}"));
        params.push(serde_json::Value::String(scopes.join(",")));
        idx += 1;
    }
    if let Some(pkce) = req.pkce_required {
        sets.push(format!("pkce_required = ${idx}"));
        params.push(serde_json::Value::Bool(pkce));
        idx += 1;
    }
    if let Some(desc) = &req.description {
        sets.push(format!("description = ${idx}"));
        params.push(serde_json::Value::String(desc.clone()));
        idx += 1;
    }
    if let Some(status) = req.status {
        sets.push(format!("status = ${idx}"));
        params.push(serde_json::Value::Number(status.into()));
        idx += 1;
    }

    if sets.is_empty() {
        return Err(Error::BusinessError("未提供任何更新字段".to_string()));
    }

    sets.push("update_time = NOW()".to_string());
    let sql = format!(
        "UPDATE cmx_auth_client SET {} WHERE client_id = $1 AND archived = 0",
        sets.join(", ")
    );

    let affected = db_manager
        .execute_sql_with_json(&db_id, None, &sql, serde_json::Value::Array(params))
        .await
        .map_err(|e| Error::InternalError(format!("更新 OAuth2 客户端失败: {e}")))?;

    if affected == 0 {
        warn!("OAuth2 客户端不存在或已归档: {}", req.client_id);
        return Err(Error::BusinessError(format!(
            "OAuth2 客户端不存在或已归档: {}",
            req.client_id
        )));
    }

    info!(client_id = req.client_id, "OAuth2 客户端更新成功");
    Ok(Json(ApiResp::ok(())))
}

/// 删除 OAuth2 客户端（软删除）
#[utoipa::path(
    post,
    path = "/api/auth/oauth2-clients/delete",
    responses(
        (status = 200, description = "删除成功")
    ),
    tag = "Auth-OAuth2Client"
)]
pub async fn delete_oauth2_client(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Json(req): Json<serde_json::Value>,
) -> Result<Json<ApiResp<()>>> {
    let client_id = req
        .get("client_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::BusinessError("缺少 client_id 参数".to_string()))?;

    debug!(
        "{:<12} - handler::delete_oauth2_client - client_id: {}",
        "HANDLER", client_id
    );

    let db_manager = cmx_database::get_default_db_manager();
    let db_id = db_manager.get_default_db_id().await;

    let sql = "UPDATE cmx_auth_client SET archived = 1, update_time = NOW() WHERE client_id = $1 AND archived = 0";
    let params = serde_json::Value::Array(vec![serde_json::Value::String(client_id.to_string())]);
    let affected = db_manager
        .execute_sql_with_json(&db_id, None, sql, params)
        .await
        .map_err(|e| Error::InternalError(format!("删除 OAuth2 客户端失败: {e}")))?;

    if affected == 0 {
        return Err(Error::BusinessError(format!(
            "OAuth2 客户端不存在: {client_id}"
        )));
    }

    info!(client_id = client_id, "OAuth2 客户端已删除");
    Ok(Json(ApiResp::ok(())))
}
