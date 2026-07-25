//! 数据字典（DCT）数据装载/回存 HTTP handler —— 薄 axum 层。
//!
//! 提取参数 → `store::resolve_dict` 解析字典视图 → 调 `cmx_dct_store_pg` 服务函数 →
//! `ApiResp`/msgpack 信封。端点：
//!   - `GET  /api/dct/meta`                    —— 字典显示元数据（列 caption/类型/PK/是否自分级）
//!   - `GET|POST /api/dct/data/search`         —— 装载字典数据（flat / 自分级 children，分页）
//!   - `GET|POST /api/dct/data/tokio-zmc-msgpack` —— 零拷贝装载（ZmcDataSet + 列式 msgpack 二进制）
//!   - `POST /api/dct/entries`                 —— 回存（upsert，merge 语义）
//!   - `DELETE /api/dct/entries/{id}`          —— 删除一行
//!   - `POST /api/dct/save`                    —— 基于 changeset 的回存（事务 + 乐观锁 409）

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use serde_json::{Value, json};
use tracing::debug;

use cmx_api::CmxAppState;
use cmx_api::middleware::CmxSvrContext;
use cmx_api::{ApiResp, Result};

use cmx_dct_model::DctQuery;
use cmx_dct_store_pg as store;

/// 从请求头取 db_id（字符串），交给 store::resolve_db_id 路由（缺失回退业务库）。
async fn db_id_from(headers: &HeaderMap) -> String {
    let hv = headers.get("db_id").and_then(|h| h.to_str().ok());
    store::resolve_db_id(hv).await
}

// ============================================================================
// 1) GET /api/dct/meta —— 字典显示元数据
// ============================================================================

pub async fn dct_meta(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Query(q): Query<DctQuery>,
    _headers: HeaderMap,
) -> Result<Json<ApiResp<Value>>> {
    debug!("{:<12} - dct_meta {}/{}", "HANDLER", q.module, q.dict);
    // dct_meta 是唯一需要字段完整属性（width/visible/pattern/enumValues 等）的场景：
    // 供前端字典维护页构建列模型（编辑/校验/布局）。按 ?with_props=true 按需下发扁平键，
    // 避免基本场景 payload 膨胀。
    let view = store::resolve_dict(&q, q.with_props).await?;
    let cols: Vec<Value> = view
        .columns
        .iter()
        .map(cmx_dct_model::project_meta_column)
        .collect();
    Ok(Json(ApiResp::ok(json!({
        "dictCode": view.dict_code,
        "dictName": view.dict_name,
        "tableName": view.table_name,
        "idField": view.id_field,
        "codeField": view.code_field,
        "labelField": view.label_field,
        "parentField": view.parent_field,
        "selfHierarchy": view.self_hierarchy,
        "pk": view.pk,
        "columns": cols,
    }))))
}

// ============================================================================
// 2) GET|POST /api/dct/data/search —— 装载字典数据
// ============================================================================

pub async fn dct_search(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Query(q): Query<DctQuery>,
    headers: HeaderMap,
    body: Option<Json<Value>>,
) -> Result<Json<ApiResp<Value>>> {
    let db_id = db_id_from(&headers).await;
    let view = store::resolve_dict(&q, false).await?;
    let raw = body.map(|b| b.0).unwrap_or_else(|| json!({}));
    debug!(
        "{:<12} - dct_search {} table={}",
        "HANDLER", q.dict, view.table_name
    );
    let data = store::search(&view, &raw, &db_id).await?;
    Ok(Json(ApiResp::ok(data)))
}

// ============================================================================
// 2b) GET|POST /api/dct/data/tokio-zmc-msgpack —— 零拷贝装载：tokio-postgres + ZmcDataSet
//     + 列式 msgpack 二进制出口（对标 doc 的 tokio-zmc-msgpack）。
// ============================================================================

pub async fn dct_search_zmc_msgpack(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Query(q): Query<DctQuery>,
    headers: HeaderMap,
    body: Option<Json<Value>>,
) -> Result<axum::response::Response> {
    use axum::response::IntoResponse;
    let db_id = db_id_from(&headers).await;
    let view = store::resolve_dict(&q, false).await?;
    let raw = body.map(|b| b.0).unwrap_or_else(|| json!({}));
    debug!(
        "{:<12} - dct zmc-msgpack {} table={}",
        "HANDLER", q.dict, view.table_name
    );

    let buf = store::search_zmc(&view, &raw, &db_id).await?;
    let envelope = encode_envelope_ok(&buf);
    Ok((
        [(axum::http::header::CONTENT_TYPE, "application/x-msgpack")],
        envelope,
    )
        .into_response())
}

/// 成功信封的 msgpack 字节：`{code:0, msg:"success", data:<列式包字节>}`（对标 doc）。
///
/// `rmp::encode` 的写入方法只在 buf 写入失败时返回 Err（Vec 写入不会失败），
/// 故用 expect 表达「固定结构写入不可能失败」的断言。
fn encode_envelope_ok(data_msgpack: &[u8]) -> Vec<u8> {
    use rmp::encode as mp;
    let mut buf = Vec::with_capacity(data_msgpack.len() + 32);
    mp::write_map_len(&mut buf, 3).expect("msgpack 写 map_len 不应失败");
    mp::write_str(&mut buf, "code").expect("msgpack 写 str 不应失败");
    mp::write_uint(&mut buf, 0).expect("msgpack 写 uint 不应失败");
    mp::write_str(&mut buf, "msg").expect("msgpack 写 str 不应失败");
    mp::write_str(&mut buf, "success").expect("msgpack 写 str 不应失败");
    mp::write_str(&mut buf, "data").expect("msgpack 写 str 不应失败");
    buf.extend_from_slice(data_msgpack);
    buf
}

// ============================================================================
// 3) POST /api/dct/entries —— 回存（upsert，merge 语义）
// ============================================================================

pub async fn dct_upsert(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Query(q): Query<DctQuery>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<ApiResp<Value>>> {
    let db_id = db_id_from(&headers).await;
    let view = store::resolve_dict(&q, false).await?;
    debug!(
        "{:<12} - dct_upsert {} table={}",
        "HANDLER", q.dict, view.table_name
    );

    match store::upsert(&view, body, &db_id).await? {
        store::UpsertOutcome::Invalid(violations) => Ok(Json(validation_fail_resp(&violations))),
        store::UpsertOutcome::Ok { affected, id_map } => Ok(Json(ApiResp::ok(
            json!({ "count": affected, "idMap": id_map }),
        ))),
    }
}

/// 构造校验失败响应：`{code:422, msg, data:{violations:[...]}}`（结构化，前端逐行逐列高亮）。
fn validation_fail_resp(violations: &[cmx_biz::errcode::Violation]) -> ApiResp<Value> {
    ApiResp::fail_with_data(
        422,
        format!("数据校验未通过（{} 处）", violations.len()),
        json!({ "violations": violations }),
    )
}

// ============================================================================
// 4) DELETE /api/dct/entries/{id} —— 删除一行
// ============================================================================

pub async fn dct_delete(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Query(q): Query<DctQuery>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<ApiResp<Value>>> {
    let db_id = db_id_from(&headers).await;
    let view = store::resolve_dict(&q, false).await?;
    debug!("{:<12} - dct_delete {} id={}", "HANDLER", q.dict, id);
    let data = store::delete(&view, &id, &db_id).await?;
    Ok(Json(ApiResp::ok(data)))
}

// ============================================================================
// 5) POST /api/dct/save —— 基于 changeset 的回存（对标 doc 的 ChangeSetCollector/DocSaver）。
//     body: { saveMode:"merge", changes: { <tableName|dict>: { inserted:[{id,fields}],
//             updated:[{id,fields,baseline}], deleted:[ids] } } }
//     事务内执行；updated 带 update_time baseline 做乐观锁（冲突→409）。
//     返回 { ok, mode, affected, updatedAt:[{id,updateTime}] }。
// ============================================================================

pub async fn dct_save(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Query(q): Query<DctQuery>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<axum::response::Response> {
    use axum::response::IntoResponse;
    let db_id = db_id_from(&headers).await;
    let view = store::resolve_dict(&q, false).await?;
    let save_mode = body
        .get("saveMode")
        .and_then(|v| v.as_str())
        .unwrap_or("merge")
        .to_string();
    debug!(
        "{:<12} - dct_save {} table={} mode={}",
        "HANDLER", q.dict, view.table_name, save_mode
    );

    match store::save(&view, &body, &db_id).await? {
        store::SaveOutcome::Invalid(violations) => {
            Ok(Json(validation_fail_resp(&violations)).into_response())
        }
        store::SaveOutcome::Conflict => {
            // 乐观锁冲突：返回 409（对标 doc，前端识别 conflict 提示刷新）。
            Ok((
                axum::http::StatusCode::CONFLICT,
                Json(json!({ "code": 409, "msg": "字典项已被他人修改，请刷新后重试" })),
            )
                .into_response())
        }
        store::SaveOutcome::Ok {
            affected,
            updated_at,
            id_map,
        } => Ok(Json(ApiResp::ok(json!({
            "ok": true,
            "mode": save_mode,
            "affected": affected,
            "updatedAt": updated_at,
            "idMap": id_map,
        })))
        .into_response()),
    }
}
