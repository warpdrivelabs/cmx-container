//! OAuth2 客户端数据服务（store 层）。
//!
//! 承接原 `cmx-api` 的 `handlers/auth/oauth2_client_handler.rs` 内联 SQL：把对
//! `cmx_auth_client` 物理表的读写下沉到本层。SQL 文本、表名、列清单、参数顺序与迁移前
//! **完全一致**。HTTP 语义的错误映射（如 duplicate → “client_id 已存在”）仍留在 handler，
//! 本层返回原始错误串，由 handler 决定如何呈现。

use cmx_core::model::data::dataset::DataSet;
use serde_json::Value;

/// 插入一条 OAuth2 客户端（status=1，archived=0）。
///
/// `params` 为已按占位 $1..$10 顺序构造好的 JSON 数组（handler 负责 secret 哈希/字段序列化）。
/// 返回原始错误串（不做 duplicate 判定），由 handler 映射为「client_id 已存在」等 HTTP 语义。
pub async fn insert_client(params: Value) -> Result<(), String> {
    let db_manager = cmx_database::get_default_db_manager();
    let db_id = db_manager.get_default_db_id().await;

    let sql = r#"
        INSERT INTO cmx_auth_client (id, client_id, client_name, client_secret, client_type,
            redirect_uris, grant_types, allowed_scopes, pkce_required, description, status, archived)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, 1, 0)
    "#;
    db_manager
        .execute_sql_with_json(&db_id, None, sql, params)
        .await
        .map(|_| ())
        .map_err(|e| format!("{e}"))
}

/// 查询 OAuth2 客户端列表（archived=0；可选按 status/client_id 过滤）。
///
/// 返回原始 DataSet，行→结构体映射仍由 handler 负责。过滤子句拼接（含 `''` 转义、
/// ORDER BY create_time DESC）与迁移前**逐字一致**。返回原始 DB 错误串。
pub async fn list_clients(
    status: Option<i64>,
    client_id: Option<String>,
) -> Result<DataSet, String> {
    let db_manager = cmx_database::get_default_db_manager();
    let db_id = db_manager.get_default_db_id().await;

    let mut where_clause = String::from("archived = 0");
    if let Some(status) = status {
        where_clause.push_str(&format!(" AND status = {}", status));
    }
    if let Some(cid) = &client_id {
        where_clause.push_str(&format!(" AND client_id = '{}'", cid.replace('\'', "''")));
    }

    let sql = format!(
        "SELECT id, client_id, client_name, client_type, redirect_uris, grant_types, \
         allowed_scopes, pkce_required, status, description, create_time, update_time \
         FROM cmx_auth_client WHERE {where_clause} ORDER BY create_time DESC"
    );

    db_manager
        .query_sql(&db_id, None, &sql, "oauth2_clients_list")
        .await
        .map_err(|e| format!("{e}"))
}

/// 执行动态字段更新（handler 已拼好 `SET` 子句片段与占位；本层套上表名/WHERE 并执行）。
///
/// `set_clause` 形如 `client_name = $2, ..., update_time = NOW()`；`params[0]` 为 client_id。
/// 返回受影响行数（0 = 不存在/已归档）。SQL 结构与迁移前一致。返回原始 DB 错误串。
pub async fn update_client(set_clause: &str, params: Value) -> Result<u64, String> {
    let db_manager = cmx_database::get_default_db_manager();
    let db_id = db_manager.get_default_db_id().await;

    let sql = format!(
        "UPDATE cmx_auth_client SET {} WHERE client_id = $1 AND archived = 0",
        set_clause
    );
    db_manager
        .execute_sql_with_json(&db_id, None, &sql, params)
        .await
        .map_err(|e| format!("{e}"))
}

/// 软删除 OAuth2 客户端（archived=1）。返回受影响行数（0 = 不存在/已归档）。返回原始 DB 错误串。
pub async fn soft_delete_client(client_id: &str) -> Result<u64, String> {
    let db_manager = cmx_database::get_default_db_manager();
    let db_id = db_manager.get_default_db_id().await;

    let sql = "UPDATE cmx_auth_client SET archived = 1, update_time = NOW() WHERE client_id = $1 AND archived = 0";
    let params = Value::Array(vec![Value::String(client_id.to_string())]);
    db_manager
        .execute_sql_with_json(&db_id, None, sql, params)
        .await
        .map_err(|e| format!("{e}"))
}
