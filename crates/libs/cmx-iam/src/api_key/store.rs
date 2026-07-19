//! API Key 数据服务（store 层）。
//!
//! 承接原 `cmx-api` 的 `handlers/auth/api_key_handler.rs` 内联 SQL：把对
//! `cmx_auth_api_key` 物理表的读写下沉到本层，让 HTTP handler 回归纯适配（提取 →
//! 调 store → 组装响应）。SQL 文本、表名、列清单、参数顺序与迁移前**完全一致**。
//!
//! Redis 缓存失效仍留在 handler（属 HTTP 侧关注点），本层只负责 DB。

use cmx_core::model::cell::DataValue;
use cmx_core::model::data::dataset::DataSet;

/// 根据 id 查询 API Key 的 key_prefix（用于删除/禁用后失效 Redis 缓存）。
///
/// 查询失败返回 `None`（与迁移前 `.ok()?` 语义一致，调用方据此决定是否失效缓存）。
pub async fn query_key_prefix_by_id(id: &str) -> Option<String> {
    let db_manager = cmx_database::get_default_db_manager();
    let db_id = db_manager.get_default_db_id().await;
    let sql = "SELECT key_prefix FROM cmx_auth_api_key WHERE id = $1";
    let params = vec![DataValue::String(id.to_string())];
    let dataset = db_manager
        .query_sql_with_datavalues(&db_id, None, sql, params, "api_key_prefix")
        .await
        .ok()?;
    let schema = dataset.schema.as_ref();
    dataset
        .iter()
        .next()
        .and_then(|row| row.get_by_name_as(schema, "key_prefix"))
}

/// 插入一条 API Key（status=1 启用，archived=0）。
///
/// 参数已由 handler 预处理（id 铸号、key_prefix 截取、key_hash、scopes_json 序列化）。
/// 返回原始 DB 错误串（不加业务前缀），由 handler 拼接与迁移前一致的错误消息。
#[allow(clippy::too_many_arguments)]
pub async fn insert_api_key(
    id: &str,
    key_prefix: &str,
    key_hash: &str,
    user_id: Option<String>,
    service_name: Option<String>,
    scopes_json: String,
    description: Option<String>,
) -> std::result::Result<(), String> {
    let db_manager = cmx_database::get_default_db_manager();
    let db_id = db_manager.get_default_db_id().await;

    let sql = r#"
        INSERT INTO cmx_auth_api_key (id, key_prefix, key_hash, user_id, service_name, scopes, description, status, archived)
        VALUES ($1, $2, $3, $4, $5, $6, $7, 1, 0)
    "#;
    let params = vec![
        DataValue::String(id.to_string()),
        DataValue::String(key_prefix.to_string()),
        DataValue::String(key_hash.to_string()),
        user_id.map(DataValue::String).unwrap_or(DataValue::Null),
        service_name
            .map(DataValue::String)
            .unwrap_or(DataValue::Null),
        DataValue::String(scopes_json),
        description
            .map(DataValue::String)
            .unwrap_or(DataValue::Null),
    ];
    db_manager
        .execute_sql_with_datavalues(&db_id, None, sql, params)
        .await
        .map(|_| ())
        .map_err(|e| format!("{e}"))
}

/// 查询 API Key 列表（archived=0；可选按 status/user_id/service_name 过滤）。
///
/// 返回原始 DataSet，行→结构体的映射仍由 handler 负责（保持字段选择与列名一致）。
/// 过滤子句拼接（含 `''` 转义、ORDER BY create_time DESC）与迁移前**逐字一致**。
/// 返回原始 DB 错误串（不加业务前缀），由 handler 拼接错误消息。
pub async fn list_api_keys(
    status: Option<i64>,
    user_id: Option<String>,
    service_name: Option<String>,
) -> std::result::Result<DataSet, String> {
    let db_manager = cmx_database::get_default_db_manager();
    let db_id = db_manager.get_default_db_id().await;

    let mut where_clause = String::from("archived = 0");
    if let Some(status) = status {
        where_clause.push_str(&format!(" AND status = {}", status));
    }
    if let Some(uid) = &user_id {
        where_clause.push_str(&format!(" AND user_id = '{}'", uid.replace('\'', "''")));
    }
    if let Some(svc) = &service_name {
        where_clause.push_str(&format!(
            " AND service_name = '{}'",
            svc.replace('\'', "''")
        ));
    }

    let sql = format!(
        "SELECT id, key_prefix, user_id, service_name, scopes, description, status, create_time \
         FROM cmx_auth_api_key WHERE {where_clause} ORDER BY create_time DESC"
    );

    db_manager
        .query_sql(&db_id, None, &sql, "api_keys_list")
        .await
        .map_err(|e| format!("{e}"))
}

/// 删除一条 API Key（按 id，archived=0）。返回受影响行数（0 = 不存在）。
/// 返回原始 DB 错误串（不加业务前缀），由 handler 拼接错误消息。
pub async fn delete_api_key(id: &str) -> std::result::Result<u64, String> {
    let db_manager = cmx_database::get_default_db_manager();
    let db_id = db_manager.get_default_db_id().await;

    let sql = "DELETE FROM cmx_auth_api_key WHERE id = $1 AND archived = 0";
    let params = vec![DataValue::String(id.to_string())];
    db_manager
        .execute_sql_with_datavalues(&db_id, None, sql, params)
        .await
        .map_err(|e| format!("{e}"))
}

/// 切换 API Key 启用状态（按 id，archived=0；同时刷新 update_time）。返回受影响行数。
/// 返回原始 DB 错误串（不加业务前缀），由 handler 拼接错误消息。
pub async fn set_api_key_status(id: &str, status: i64) -> std::result::Result<u64, String> {
    let db_manager = cmx_database::get_default_db_manager();
    let db_id = db_manager.get_default_db_id().await;

    let sql = "UPDATE cmx_auth_api_key SET status = $2, update_time = NOW() WHERE id = $1 AND archived = 0";
    let params = vec![DataValue::String(id.to_string()), DataValue::Int(status)];
    db_manager
        .execute_sql_with_datavalues(&db_id, None, sql, params)
        .await
        .map_err(|e| format!("{e}"))
}
