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

use cmx_api_core::CmxAppState;
use cmx_api_core::db_id::resolve_db_id_from_headers;
use cmx_api_core::middleware::CmxSvrContext;
use cmx_api_core::msgpack::msgpack_ok_response;
use cmx_biz::errcode::validation_fail_resp;
use cmx_api_core::{ApiResp, Result};

use cmx_dct_store_pg::{self as store, BatchConflictMode, DctQuery, SearchQuery, Txn};

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

/// 返回字典显示元数据。
///
/// `GET /api/dct/meta` —— 按 `DctQuery` 坐标解析字典定义（DAM / file 可缺省，后端自动反查补全），投影出列
/// caption、类型、主键、是否自分级等显示元数据。`?with_props=true` 时
/// `columns[].extra` 额外携带字段扁平属性（width / visible / pattern / enumValues /
/// required / intDigits / decimalDigits 等），供字典数据维护页编辑、校验与布局使用。
#[utoipa::path(
    get,
    path = "/api/dct/meta",
    params(DctQuery),
    responses(
        (status = 200, description = "字典显示元数据（列 caption / 类型 / PK / 是否自分级）；with_props=true 时含完整扁平属性", body = ApiResp<Value>)
    ),
    tag = "DCT字典接口"
)]
pub async fn dct_meta(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Query(q): Query<DctQuery>,
    _headers: HeaderMap,
) -> Result<Json<ApiResp<Value>>> {
    debug!("{:<12} - dct_meta {}/{}", "HANDLER", q.module.as_deref().unwrap_or("(auto)"), q.dict);
    // dict_meta 场景函数已下沉投影（DictMeta derive Serialize + camelCase），
    // handler 直接 to_value 包进 ApiResp，无需手动拼 json!。
    // with_props 从 q 读取：?with_props=true 时 columns 含完整扁平属性（供字典维护页）。
    let meta = store::dict_meta(&q).await?;
    Ok(Json(ApiResp::ok(serde_json::to_value(&meta)?)))
}

// ============================================================================
// 2) GET|POST /api/dct/data/search —— 装载字典数据
// ============================================================================

/// 装载字典数据。
///
/// `GET|POST /api/dct/data/search` —— flat / 自分级 children，分页。
/// GET 仅按 `DctQuery` 定位，使用默认分页（page=1、pageSize=500）；POST 时 body 为
/// SearchQuery JSON：
///
/// ```json
/// {
///   "filters": { "code": "001", "status": ["active", "pending"], "remark": null },
///   "q": "模糊搜索词",
///   "sort": { "field": "code", "order": "desc" },
///   "page": 1,
///   "pageSize": 500,
///   "parentId": "自分级字典的父行 id"
/// }
/// ```
///
/// `filters` 值语义：标量 = 等值、数组 = IN、null = IS NULL；`pageSize` 上限 5000。
/// 自分级字典传 `parentId` 时装载该父行下的 children。
#[utoipa::path(
    post,
    path = "/api/dct/data/search",
    params(DctQuery),
    request_body = Value,
    responses(
        (status = 200, description = "字典数据：rows（行数组）+ total / page / pageSize 分页信息", body = ApiResp<Value>)
    ),
    tag = "DCT字典接口"
)]
pub async fn dct_search(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Query(q): Query<DctQuery>,
    headers: HeaderMap,
    body: Option<Json<Value>>,
) -> Result<Json<ApiResp<Value>>> {
    let db_id = resolve_db_id_from_headers(&headers).await;
    let search = SearchQuery::from_body(body.map(|b| b.0));
    debug!("{:<12} - dct_search {}", "HANDLER", q.dict);
    let result = store::dict_search(&q, &search, &db_id).await?;
    Ok(Json(ApiResp::ok(json!({
        "rows": result.rows,
        "total": result.total,
        "page": result.page,
        "pageSize": result.page_size,
    }))))
}

// ============================================================================
// 2b) GET|POST /api/dct/data/tokio-zmc-msgpack —— 零拷贝装载：tokio-postgres + ZmcDataSet
//     + 列式 msgpack 二进制出口（对标 doc 的 tokio-zmc-msgpack）。
// ============================================================================

/// 零拷贝装载字典数据。
///
/// `GET|POST /api/dct/data/tokio-zmc-msgpack` —— 列式 msgpack 二进制出口。
/// tokio-postgres + ZmcDataSet 零拷贝链路，装载语义与 `/api/dct/data/search` 一致，
/// 仅出口不同：响应为列式 msgpack 二进制信封（`Content-Type: application/x-msgpack`）。
/// GET 仅按 `DctQuery` 定位（默认分页）；POST 时 body 为 SearchQuery JSON：
///
/// ```json
/// {
///   "filters": { "code": "001", "status": ["active", "pending"], "remark": null },
///   "q": "模糊搜索词",
///   "sort": { "field": "code", "order": "desc" },
///   "page": 1,
///   "pageSize": 500,
///   "parentId": "自分级字典的父行 id"
/// }
/// ```
#[utoipa::path(
    post,
    path = "/api/dct/data/tokio-zmc-msgpack",
    params(DctQuery),
    request_body = Value,
    responses(
        (status = 200, description = "列式 msgpack 二进制信封：{code, msg, data}（data 为列式编码字节）", content_type = "application/x-msgpack")
    ),
    tag = "DCT字典接口"
)]
pub async fn dct_search_zmc_msgpack(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Query(q): Query<DctQuery>,
    headers: HeaderMap,
    body: Option<Json<Value>>,
) -> Result<axum::response::Response> {
    let db_id = resolve_db_id_from_headers(&headers).await;
    let search = SearchQuery::from_body(body.map(|b| b.0));
    debug!("{:<12} - dct zmc-msgpack {}", "HANDLER", q.dict);

    // dict_search_zmc 返回原始 ZmcDataSet；此处编码为列式二进制 + msgpack 信封。
    let zmc = store::dict_search_zmc(&q, &search, &db_id).await?;
    let mut buf = Vec::new();
    zmc.encode_columnar_binary(&mut buf);
    Ok(msgpack_ok_response(&buf))
}

// ============================================================================
// 3) POST /api/dct/entries —— 回存（upsert，merge 语义）
// ============================================================================

/// 回存字典条目。
///
/// `POST /api/dct/entries` —— upsert，merge 语义。
/// body 为行数组 JSON（或单行对象），每行是按字典列取值的扁平对象：带主键的行
/// 更新，无主键（或临时 id）的行铸号插入：
///
/// ```json
/// [
///   { "id": "temp-1", "code": "CNY", "name": "人民币" },
///   { "code": "USD", "name": "美元" }
/// ]
/// ```
///
/// 落库前做列级校验（类型 / 长度 / 精度 / 非空），一次回报全部违规；配置了编码
/// 引擎（codeRule）的字典会为 code 为空的新行自动铸业务编码。
#[utoipa::path(
    post,
    path = "/api/dct/entries",
    params(DctQuery),
    request_body = Value,
    responses(
        (status = 200, description = "回存结果 {count, idMap}（idMap：临时 id → 真号）；校验失败返回结构化 violations", body = ApiResp<Value>)
    ),
    tag = "DCT字典接口"
)]
pub async fn dct_upsert(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Query(q): Query<DctQuery>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<ApiResp<Value>>> {
    let db_id = resolve_db_id_from_headers(&headers).await;
    debug!("{:<12} - dct_upsert {}", "HANDLER", q.dict);

    match store::dict_upsert(&q, body, &db_id, Txn::Auto).await? {
        store::UpsertOutcome::Invalid(violations) => Ok(Json(validation_fail_resp(&violations))),
        store::UpsertOutcome::Ok { affected, id_map } => Ok(Json(ApiResp::ok(
            json!({ "count": affected, "idMap": id_map }),
        ))),
    }
}

// ============================================================================
// 4) DELETE /api/dct/entries/{id} —— 删除一行
// ============================================================================

/// 删除一行字典条目。
///
/// `DELETE /api/dct/entries/{id}` —— 按路径参数 `id`（主键）删除 `DctQuery` 定位的字典表中的一行；连号域
/// （enable_gap）字典删除前记录断号。既有接口，保留 DELETE 方法与路径参数
/// （新接口规范不再如此设计）。
#[utoipa::path(
    delete,
    path = "/api/dct/entries/{id}",
    params(
        ("id" = String, Path, description = "要删除的字典条目主键"),
        DctQuery
    ),
    responses(
        (status = 200, description = "删除结果 {ok, deleted}（deleted 为实际删除行数）", body = ApiResp<Value>)
    ),
    tag = "DCT字典接口"
)]
pub async fn dct_delete(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Query(q): Query<DctQuery>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<ApiResp<Value>>> {
    let db_id = resolve_db_id_from_headers(&headers).await;
    debug!("{:<12} - dct_delete {} id={}", "HANDLER", q.dict, id);
    let data = store::dict_delete(&q, &id, &db_id, Txn::Auto).await?;
    Ok(Json(ApiResp::ok(data)))
}

// ============================================================================
// 5) POST /api/dct/save —— 基于 changeset 的回存（对标 doc 的 ChangeSetCollector/DocSaver）。
//     body: { saveMode:"merge", changes: { <tableName|dict>: { inserted:[{id,fields}],
//             updated:[{id,fields,baseline}], deleted:[ids] } } }
//     事务内执行；updated 带 update_time baseline 做乐观锁（冲突→409）。
//     返回 { ok, mode, affected, updatedAt:[{id,updateTime}] }。
// ============================================================================

/// Changeset 回存字典。
///
/// `POST /api/dct/save` —— 事务 + 乐观锁。body 为 changeset JSON：
///
/// ```json
/// {
///   "saveMode": "merge",
///   "changes": {
///     "<字典表名>": {
///       "inserted": [ { "id": "临时 id", "fields": { "code": "...", "name": "..." } } ],
///       "updated": [ { "id": "...", "fields": { "name": "..." }, "baseline": { "update_time": "..." } } ],
///       "deleted": [ "id1", "id2" ]
///     }
///   }
/// }
/// ```
///
/// `saveMode` 缺省 `merge`（按 changes 增量回存）；`replace` 时改用 `snapshot`
/// 整表替换。`updated` 行以 `baseline.update_time` 做乐观锁，他人已修改时返回
/// HTTP 409，前端提示刷新后重试。落库前做列级校验，一次回报全部违规。
#[utoipa::path(
    post,
    path = "/api/dct/save",
    params(DctQuery),
    request_body = Value,
    responses(
        (status = 200, description = "保存结果 {ok, mode, affected, updatedAt: [{id, updateTime}], idMap}", body = ApiResp<Value>),
        (status = 409, description = "乐观锁冲突（update_time baseline 不匹配，他人已修改），前端提示刷新后重试", body = ApiResp<Value>)
    ),
    tag = "DCT字典接口"
)]
pub async fn dct_save(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Query(q): Query<DctQuery>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<axum::response::Response> {
    use axum::response::IntoResponse;
    let db_id = resolve_db_id_from_headers(&headers).await;
    let save_mode = body
        .get("saveMode")
        .and_then(|v| v.as_str())
        .unwrap_or("merge")
        .to_string();
    debug!(
        "{:<12} - dct_save {} mode={}",
        "HANDLER", q.dict, save_mode
    );

    match store::dict_save(&q, &body, &db_id, Txn::Auto).await? {
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
#[derive(serde::Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ExportParams {
    /// 导出格式：`json`（默认，NDJSON）/ `csv`
    #[serde(default = "default_export_format")]
    pub format: String,
}

fn default_export_format() -> String {
    "json".to_string()
}

/// 流式导出字典全表。
///
/// `GET /api/dct/export` —— 支持 NDJSON / CSV 格式，附件下载。
///
/// - 走 keyset 分页（`WHERE pk > $last_pk ORDER BY pk LIMIT N`）+ mpsc +
///   `Body::from_stream`，逐行流式吐出，服务端内存平稳
/// - 响应头：`Content-Type`（随 format：NDJSON / CSV）+ `Content-Disposition: attachment`
/// - 文件名 `{dict_code}_{table_name}.{ext}`（纯 ASCII，无乱码风险）
#[utoipa::path(
    get,
    path = "/api/dct/export",
    params(DctQuery, ExportParams),
    responses(
        (status = 200, description = "流式导出全表（format=json 为 NDJSON，format=csv 为 CSV）；Content-Disposition: attachment，文件名 {dictCode}_{tableName}.{ext}", content_type = "application/x-ndjson")
    ),
    tag = "DCT字典接口"
)]
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
    let fmt = match params.format.to_lowercase().as_str() {
        "csv" => store::ImportFormat::Csv,
        _ => store::ImportFormat::Json,
    };

    debug!(
        "{:<12} - dct_export {} fmt={}",
        "HANDLER", q.dict, fmt.as_str()
    );

    // 启动导出流：mpsc::Receiver<Bytes>（内部 spawn tokio task 跑 keyset 分页）。
    // export_stream 现在接 &DctQuery（内部 resolve_dict），返回 Result<Receiver>。
    let rx = store::export_stream(&q, db_id, fmt, 5000, 8).await?;

    // 包装为 axum Body：用 SafeReceiverStream（channel 关闭后再次 poll 安全返回 None，不 panic）。
    // 不能用 futures::stream::unfold：unfold 在返回 Ready(None) 后会 panic（hyper 1.x 可能再次 poll）。
    let stream = SafeReceiverStream::new(rx);
    let body = Body::from_stream(stream);

    // 文件名需 dict_code + table_name：再查一次 meta（轻量，定义已缓存）。
    let meta = store::dict_meta(&q).await?;
    let filename = format!(
        "{}_{}.{}",
        meta.dict_code,
        meta.table_name,
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

/// 导入请求的 query 参数（字典定位仍走 `DctQuery`）。
#[derive(serde::Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ImportParams {
    /// 写入语义：upsert（默认）/ replace / insert_only；multipart `mode` 字段优先覆盖。
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
/// `POST /api/dct/import` —— multipart/form-data 上传：
/// - `file`：文件字段（filename + content_type 自动识别 CSV / JSON NDJSON）
/// - `mode`：可选字符串字段（`upsert` / `replace` / `insert_only`，默认 `upsert`；
///   URL query `?mode=` 亦可传，multipart 字段优先覆盖）
///
/// 返回导入汇总 `ImportSummary`：`{total, affected, skipped, errors}`。
#[utoipa::path(
    post,
    path = "/api/dct/import",
    params(DctQuery, ImportParams),
    request_body(content = Value, description = "multipart/form-data：file（文件字段，filename + content-type 用于格式识别）+ mode（可选字段：upsert / replace / insert_only，默认 upsert）。自动识别 .csv / .json / .ndjson", content_type = "multipart/form-data"),
    responses(
        (status = 200, description = "流式导入完成，返回 {total, affected, skipped, errors}", body = ApiResp<Value>)
    ),
    tag = "DCT字典接口"
)]
pub async fn dct_import(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Query(q): Query<DctQuery>,
    headers: HeaderMap,
    Query(params): Query<ImportParams>,
    mut multipart: Multipart,
) -> Result<Json<ApiResp<Value>>> {
    let db_id = resolve_db_id_from_headers(&headers).await;

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
        "{:<12} - dct_import {} fmt={:?} mode={:?}",
        "HANDLER", q.dict, fmt, mode
    );

    // 用 std::io::Cursor 包装 bytes（tokio 为 std::io::Cursor<T: AsRef<[u8]> + Unpin>
    // 实现了 AsyncRead，Bytes 满足 AsRef<[u8]> + Unpin）。
    let cursor = std::io::Cursor::new(bytes);
    let summary = store::import_stream(&q, db_id, fmt, mode, 1000, cursor).await?;

    Ok(Json(ApiResp::ok(json!({
        "total": summary.total,
        "affected": summary.affected,
        "skipped": summary.skipped,
        "errors": summary.errors,
    }))))
}
