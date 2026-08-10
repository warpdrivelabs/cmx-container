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
//!   - `GET  /api/dct/export`                  —— 流式导出全表（JSON NDJSON / CSV）
//!   - `POST /api/dct/import`                  —— 流式导入（multipart/form-data：file + mode）

use axum::Json;
use axum::extract::{Multipart, Path, Query, State};
use axum::http::HeaderMap;
use serde_json::{Value, json};
use std::task::{Context, Poll};
use tracing::debug;

use cmx_api::CmxAppState;
use cmx_api::db_id::resolve_db_id_from_headers;
use cmx_api::middleware::CmxSvrContext;
use cmx_api::{ApiResp, Result};

use cmx_dct_model::DctQuery;
use cmx_dct_model::BatchConflictMode;
use cmx_dct_store_pg as store;

/// 把 `tokio::sync::mpsc::Receiver<Bytes>` 包成 `Stream<Item = Result<Bytes, io::Error>>`。
///
/// **为什么不直接用 `futures::stream::unfold`**：unfold 在 future 返回 `None` 后会把 state
/// 设为 `Done`，再次 poll 会 panic（`Unfold must not be polled after it returned Poll::Ready(None)`）。
/// hyper 1.x 在客户端断开 / 连接清理时可能再次 poll 已结束的 stream，导致 panic。
/// 这里用 `Option<Receiver>` 显式 take，channel 关闭后下次 poll 安全返回 `Poll::Ready(None)`。
struct SafeReceiverStream {
    rx: Option<tokio::sync::mpsc::Receiver<bytes::Bytes>>,
}

impl SafeReceiverStream {
    fn new(rx: tokio::sync::mpsc::Receiver<bytes::Bytes>) -> Self {
        Self { rx: Some(rx) }
    }
}

impl futures::Stream for SafeReceiverStream {
    type Item = std::result::Result<bytes::Bytes, std::io::Error>;

    fn poll_next(mut self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let Some(rx) = self.rx.as_mut() else {
            return Poll::Ready(None);
        };
        match rx.poll_recv(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Some(chunk)) => Poll::Ready(Some(Ok(chunk))),
            Poll::Ready(None) => {
                self.rx = None; // channel 关闭，取走，下次返回 None 不 panic
                Poll::Ready(None)
            }
        }
    }
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
        "codeRule": view.code_rule,
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
    let db_id = resolve_db_id_from_headers(&headers).await;
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
    let db_id = resolve_db_id_from_headers(&headers).await;
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
    let db_id = resolve_db_id_from_headers(&headers).await;
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
    let db_id = resolve_db_id_from_headers(&headers).await;
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
    let db_id = resolve_db_id_from_headers(&headers).await;
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

// ============================================================================
// 6) GET /api/dct/export —— 流式导出全表（JSON NDJSON / CSV）
// ============================================================================

/// 导出请求的 query 参数。
#[derive(serde::Deserialize)]
pub struct ExportParams {
    /// 导出格式：`json`（默认，NDJSON）/ `csv`
    #[serde(default = "default_export_format")]
    pub format: String,
}

fn default_export_format() -> String {
    "json".to_string()
}

/// 流式导出字典全表数据。
///
/// - 走 keyset 分页（`WHERE pk > $last_pk ORDER BY pk LIMIT N`）+ mpsc + `Body::from_stream`
/// - 响应头：`Content-Type` + `Content-Disposition: attachment`
/// - 文件名 `{dict_code}_{table_name}.{ext}`（纯 ASCII，无乱码风险）
pub async fn dct_export(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Query(q): Query<DctQuery>,
    headers: HeaderMap,
    Query(params): Query<ExportParams>,
) -> Result<axum::response::Response> {
    use axum::body::Body;
    use axum::response::IntoResponse;

    let db_id = resolve_db_id_from_headers(&headers).await;
    let view = store::resolve_dict(&q, false).await?;
    let fmt = match params.format.to_lowercase().as_str() {
        "csv" => store::ImportFormat::Csv,
        _ => store::ImportFormat::Json,
    };

    debug!(
        "{:<12} - dct_export {} table={} fmt={}",
        "HANDLER", q.dict, view.table_name, fmt.as_str()
    );

    // 启动导出流：mpsc::Receiver<Bytes>（内部 spawn tokio task 跑 keyset 分页）
    let rx = store::export_stream(view.clone(), db_id, fmt, 5000, 8);

    // 包装为 axum Body：用 SafeReceiverStream（channel 关闭后再次 poll 安全返回 None，不 panic）。
    // 不能用 futures::stream::unfold：unfold 在返回 Ready(None) 后会 panic（hyper 1.x 可能再次 poll）。
    let stream = SafeReceiverStream::new(rx);
    let body = Body::from_stream(stream);

    let filename = format!(
        "{}_{}.{}",
        view.dict_code,
        view.table_name,
        fmt.ext()
    );
    let content_disposition = format!("attachment; filename=\"{}\"", filename);

    Ok((
        [
            (
                axum::http::header::CONTENT_TYPE,
                axum::http::HeaderValue::from_str(fmt.content_type())
                    .unwrap_or_else(|_| {
                        axum::http::HeaderValue::from_static("application/octet-stream")
                    }),
            ),
            (
                axum::http::header::CONTENT_DISPOSITION,
                axum::http::HeaderValue::from_str(&content_disposition)
                    .unwrap_or_else(|_| {
                        axum::http::HeaderValue::from_static("attachment")
                    }),
            ),
        ],
        body,
    )
        .into_response())
}

// ============================================================================
// 7) POST /api/dct/import —— 流式导入（multipart/form-data：file + mode）
// ============================================================================

/// 导入请求的 query 参数（mode 通过 multipart field 传，不在这里）。
/// 保留 DctQuery 用于定位字典。
#[derive(serde::Deserialize)]
pub struct ImportParams {
    /// 写入语义：upsert（默认）/ replace / insert_only（multipart `mode` 字段覆盖）
    #[serde(default = "default_import_mode")]
    pub mode: String,
}

fn default_import_mode() -> String {
    "upsert".to_string()
}

/// 自动识别导入文件格式：扩展名 → Content-Type → 默认 JSON。
///
/// - 扩展名 `.csv` → Csv；`.json` / `.ndjson` → Json
/// - Content-Type `text/csv` → Csv；`application/json` / `application/x-ndjson` → Json
/// - 都识别不出 → 默认 Json（NDJSON）
fn detect_format(filename: &str, content_type: &str) -> Result<store::ImportFormat> {
    let ext_lower = filename.rsplit('.').next().map(|s| s.to_lowercase());
    if matches!(ext_lower.as_deref(), Some("csv")) {
        return Ok(store::ImportFormat::Csv);
    }
    if matches!(ext_lower.as_deref(), Some("json") | Some("ndjson")) {
        return Ok(store::ImportFormat::Json);
    }
    let ct_lower = content_type.to_lowercase();
    if ct_lower.contains("csv") {
        return Ok(store::ImportFormat::Csv);
    }
    if ct_lower.contains("json") {
        return Ok(store::ImportFormat::Json);
    }
    // 兜底：JSON NDJSON（与导出默认格式一致）
    Ok(store::ImportFormat::Json)
}

/// 流式导入字典数据。
///
/// multipart/form-data：
/// - `file`：文件字段（filename + content_type 用于格式识别）
/// - `mode`：可选字符串字段（`upsert` / `replace` / `insert_only`，默认 `upsert`）
///
/// 返回 `ImportSummary`：`{total, affected, skipped, errors}`
pub async fn dct_import(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Query(q): Query<DctQuery>,
    headers: HeaderMap,
    Query(params): Query<ImportParams>,
    mut multipart: Multipart,
) -> Result<Json<ApiResp<Value>>> {
    let db_id = resolve_db_id_from_headers(&headers).await;
    let view = store::resolve_dict(&q, false).await?;

    let mut mode = match params.mode.as_str() {
        "replace" => BatchConflictMode::Replace,
        "insert_only" => BatchConflictMode::InsertOnly,
        _ => BatchConflictMode::Upsert,
    };
    let mut file_bytes: Option<bytes::Bytes> = None;
    let mut file_name = String::new();
    let mut file_content_type = String::new();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| cmx_biz::BizError::business(format!("multipart 解析失败: {e}")))?
    {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "mode" => {
                let mb = field.bytes().await.map_err(|e| {
                    cmx_biz::BizError::business(format!("读取 mode 失败: {e}"))
                })?;
                let mode_str = std::str::from_utf8(&mb).unwrap_or("upsert");
                mode = match mode_str {
                    "replace" => BatchConflictMode::Replace,
                    "insert_only" => BatchConflictMode::InsertOnly,
                    _ => BatchConflictMode::Upsert,
                };
            }
            "file" => {
                file_name = field.file_name().unwrap_or("").to_string();
                file_content_type = field.content_type().unwrap_or("").to_string();
                let bytes = field.bytes().await.map_err(|e| {
                    cmx_biz::BizError::business(format!("读取 file 失败: {e}"))
                })?;
                debug!(
                    "{:<12} - dct_import {} file={} size={} ct={}",
                    "HANDLER", q.dict, file_name, bytes.len(), file_content_type
                );
                file_bytes = Some(bytes);
            }
            _ => {}
        }
    }

    let bytes = file_bytes.ok_or_else(|| {
        cmx_biz::BizError::business("multipart 缺少 file 字段".to_string())
    })?;
    let fmt = detect_format(&file_name, &file_content_type)?;
    debug!(
        "{:<12} - dct_import {} table={} fmt={:?} mode={:?}",
        "HANDLER", q.dict, view.table_name, fmt, mode
    );

    // 用 std::io::Cursor 包装 bytes（tokio 为 std::io::Cursor<T: AsRef<[u8]> + Unpin>
    // 实现了 AsyncRead，Bytes 满足 AsRef<[u8]> + Unpin）。
    let cursor = std::io::Cursor::new(bytes);
    let summary = store::import_stream(view, db_id, fmt, mode, 1000, cursor).await?;

    Ok(Json(ApiResp::ok(json!({
        "total": summary.total,
        "affected": summary.affected,
        "skipped": summary.skipped,
        "errors": summary.errors,
    }))))
}
