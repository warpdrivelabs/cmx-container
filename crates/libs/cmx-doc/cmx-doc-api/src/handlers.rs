//! 业务单据装载/回存 HTTP handler（方案 Phase 4/5）
//!
//! 装载端点命名 `/api/doc/data/<驱动>-<内存模式>-<传输>`，三段一眼可辨：
//!   - 驱动：`sqlx`（PG/MySQL/SQLite）| `tokio`（tokio-postgres）
//!   - 内存模式：`dataset`（老 DataSet，全拷贝）| `zmc`（ZmcDataSet，持原始行零拷贝）
//!   - 传输：`json`（ApiResp JSON 信封）| `msgpack`（列式二进制信封）
//!
//!   驱动 sqlx|tokio × 内存 dataset|zmc × 传输 json|msgpack 的组合端点：
//!     · `sqlx-dataset-json`   sqlx + DataSet + JSON（老链路）
//!     · `tokio-zmc-msgpack`   tokio + ZmcDataSet + msgpack 二进制
//!     · `sqlx-zmc-msgpack`    sqlx + ZmcDataSet + msgpack 二进制
//!     · `tokio-zmc-json`      tokio + ZmcDataSet + 纯 JSON
//!     · `sqlx-zmc-json`       sqlx + ZmcDataSet + 纯 JSON
//! - `POST /api/doc/save` → DocSaver 双模式回存（Phase 5 接入）
//!
//! 分层：handler 层负责「读单据定义 + 解析 DocMetaView(带缓存)」，
//! 再把强类型 meta 传给 cmx-biz 的 DocLoader/DocSaver（cmx-biz 不依赖 definitions store）。

use axum::Json;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use serde::Deserialize;
use serde_json::Value;
use tracing::debug;

use cmx_core::model::data::dataset::ColumnarCodec;
use cmx_database::get_default_db_manager;
use cmx_doc_store_pg::{DocLoader, DocMetaView, DocQuery, DocRevision, DocSaver, saver};

use cmx_api_core::actor::{actor_id_i64, actor_name};
use cmx_api_core::CmxAppState;
use cmx_api_core::db_id::resolve_db_id_from_headers;
use cmx_api_core::middleware::CmxSvrContext;
use cmx_biz::errcode::validation_fail_resp;
use cmx_api_core::{ApiResp, Result};

/// `/api/doc/data/*` 装载端点共用查询参数（GET 便捷路径：URL query）。
#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct DocDataQuery {
    /// 域；可选：缺失时后端按 doc(moduleCode)/file 全局反查（多 DAM 冲突返回 409 + 候选列表）。
    #[serde(default)]
    pub domain: Option<String>,
    /// 应用；可选：同 `domain`，缺失时全局反查补全。
    #[serde(default)]
    pub application: Option<String>,
    /// 模块；可选：同 `domain`，缺失时全局反查补全。
    #[serde(default)]
    pub module: Option<String>,
    /// 单据定义文件名（如 cmxfico_doc_meta_v1.json）；缺失时由 [`resolve_doc_file`]
    /// 在 domain/app/module 下自动选默认/最高版本。
    #[serde(default)]
    pub file: Option<String>,
    /// 单据模块编码（moduleMeta.moduleCode，取值通常为定义文件 stem 中 `_doc_meta` 前段，如 cmxfico / gl_md）；缺失时盲选默认文件，有值时按 moduleCode 精确定位。
    #[serde(default)]
    pub doc: Option<String>,
    /// GET 便捷：根层过滤 `col:value`（简单等值）
    #[serde(default)]
    pub filter: Option<String>,
    /// GET 便捷：根层限制行数
    #[serde(default)]
    pub limit: Option<u64>,
    /// 可选：装载深度（懒下钻）
    #[serde(default)]
    pub depth: Option<usize>,
}

/// 数据库驱动（内存模式一律 zmc/dataset 由 transport 决定；此处只分驱动）。
#[derive(Clone, Copy)]
enum Driver {
    Sqlx,
    Tokio,
}
/// 内存模式 + 出口传输。
#[derive(Clone, Copy)]
enum Exit {
    /// sqlx 老 DataSet + JSON
    DatasetJson,
    /// ZmcDataSet + msgpack 二进制
    ZmcMsgpack,
    /// ZmcDataSet + 纯 JSON
    ZmcJson,
}

/// 从 GET query 构造简单 DocQuery（根层等值 + limit + depth）。
fn simple_doc_query(meta: &DocMetaView, q: &DocDataQuery) -> DocQuery {
    let root_id = meta.root_layer().map(|l| l.id.clone()).unwrap_or_default();
    let mut dq = DocQuery::simple(&root_id, q.limit, q.depth);
    if let Some(f) = &q.filter
        && let Some((col, val)) = f.split_once(':')
    {
        // 简单等值 → 根层 filter JSON
        let filter = serde_json::json!({ col: val });
        let lq = dq.layers.entry(root_id.clone()).or_default();
        lq.filter = cmx_doc_store_pg::Filter::from_json(&filter).ok().flatten();
    }
    dq
}

/// 统一装载核心：按驱动 + 出口跑对应装载器，产出响应。
async fn run_doc_load(
    driver: Driver,
    exit: Exit,
    meta: &DocMetaView,
    db_id: &str,
    dq: &DocQuery,
) -> Result<axum::response::Response> {
    use axum::response::IntoResponse;
    use cmx_database_pg::get_default_pg_db_manager;
    use cmx_doc_store_pg::{ZmcDocLoader, ZmcDocLoaderSqlx};

    // 装载前校验列名（防注入 + 明确报错）
    dq.validate(meta)?;

    match (driver, exit) {
        // sqlx + 老 DataSet + JSON
        (Driver::Sqlx, Exit::DatasetJson) => {
            let mm = get_default_db_manager();
            let ds = DocLoader::load(mm, db_id, meta, dq).await?;
            let pkg = ColumnarCodec::encode(&ds);
            Ok(Json(ApiResp::ok(pkg)).into_response())
        }
        // tokio + Zmc + msgpack
        (Driver::Tokio, Exit::ZmcMsgpack) => {
            let mm = get_default_pg_db_manager();
            let zmc = ZmcDocLoader::load(mm, db_id, meta, dq).await?;
            let mut buf = Vec::new();
            zmc.encode_columnar_binary(&mut buf);
            Ok(msgpack_ok_response(&buf))
        }
        // sqlx + Zmc + msgpack
        (Driver::Sqlx, Exit::ZmcMsgpack) => {
            let mm = get_default_db_manager();
            let zmc = ZmcDocLoaderSqlx::load(mm, db_id, meta, dq).await?;
            let mut buf = Vec::new();
            zmc.encode_columnar_binary(&mut buf);
            Ok(msgpack_ok_response(&buf))
        }
        // tokio + Zmc + JSON
        (Driver::Tokio, Exit::ZmcJson) => {
            let mm = get_default_pg_db_manager();
            let zmc = ZmcDocLoader::load(mm, db_id, meta, dq).await?;
            Ok(Json(ApiResp::ok(zmc.encode_columnar_json())).into_response())
        }
        // sqlx + Zmc + JSON
        (Driver::Sqlx, Exit::ZmcJson) => {
            let mm = get_default_db_manager();
            let zmc = ZmcDocLoaderSqlx::load(mm, db_id, meta, dq).await?;
            Ok(Json(ApiResp::ok(zmc.encode_columnar_json())).into_response())
        }
        // 组合无意义（老 DataSet 只在 sqlx 侧）：tokio+DatasetJson 不存在
        (Driver::Tokio, Exit::DatasetJson) => {
            Err(cmx_biz::BizError::business("tokio 驱动无老 DataSet 通道").into())
        }
    }
}

/// GET 便捷 + POST 富查询共用的装载入口。
/// - GET：URL query（简单等值/limit/depth）；
/// - POST：body = 完整 [`DocQuery`] JSON（每层 filter/orderBy/分页/游标）。
///
/// 叠加规则：POST 富查询时，URL 上的便捷参数（depth/limit）作为**兜底默认值**——
/// body 显式给了的字段以 body 为准，body 没给的就回退到 URL query。
/// 这样前端既可以用 GET `?depth=1` 控制"只装根层"，也可以在 POST 富查询（分页/游标）
/// 场景下用同一个 URL 参数达到同样效果，不会被 body 静默吞掉。
async fn doc_load_entry(
    driver: Driver,
    exit: Exit,
    q: DocDataQuery,
    headers: HeaderMap,
    body: Option<Value>,
) -> Result<axum::response::Response> {
    let db_id = resolve_db_id_from_headers(&headers).await;
    let (meta, _file) = resolve_doc_meta(
        q.domain.as_deref(),
        q.application.as_deref(),
        q.module.as_deref(),
        q.file.as_deref(),
        q.doc.as_deref(),
    )
    .await?;
    let dq = match body {
        Some(b) if !b.is_null() => {
            let mut dq = DocQuery::from_json(&b)?;
            // body 未显式指定 depth 时，回退到 URL query 的 ?depth=
            if dq.depth.is_none() && q.depth.is_some() {
                dq.depth = q.depth;
            }
            dq
        }
        _ => simple_doc_query(&meta, &q),
    };
    run_doc_load(driver, exit, &meta, &db_id, &dq).await
}

// ── 五个组合端点（GET + POST 共用 doc_load_entry） ──────────────────────────

/// 装载单据数据-sqlx-dataset-json
///
/// `GET|POST /api/doc/data/sqlx-dataset-json` —— sqlx + 老 DataSet 全拷贝 + JSON 信封出口。
/// GET 走便捷路径：URL query 传坐标（domain / application / module / file / doc）+
/// `filter=列名:值` 简单等值 + `limit` + `depth`；POST 时 body 为 DocQuery 富查询
/// JSON（URL 上的 depth / limit 作为兜底默认值）：
///
/// ```json
/// {
///   "depth": 2,
///   "includeSiblings": true,
///   "countTotal": false,
///   "layers": {
///     "<层 id>": {
///       "filter": { "status": "posted", "amount": { "$gt": 0 },
///                   "$or": [ { "type": "1" }, { "type": "2" } ] },
///       "orderBy": ["!posting_date", "code"],
///       "limit": 50,
///       "offset": 0,
///       "cursor": "<base64 游标>"
///     }
///   }
/// }
/// ```
///
/// filter 语义：键值对隐式 AND；标量值 = 等值简写；支持算子 `$eq` `$ne` `$gt` `$gte`
/// `$lt` `$lte` `$like` `$ilike` `$contains` `$startsWith` `$endsWith` `$null` 及
/// `$or` / `$and` 组合；`orderBy` 元素前缀 `!` 表示降序。
#[utoipa::path(
    post,
    path = "/api/doc/data/sqlx-dataset-json",
    params(DocDataQuery),
    request_body = Value,
    responses(
        (status = 200, description = "列式包（ApiResp JSON 信封）：datasetId / columns / rows / childRows，前端 CmxDataSet.fromJSON 直接装载", body = ApiResp<Value>)
    ),
    tag = "DOC单据接口"
)]
pub async fn doc_data_sqlx_dataset_json(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Query(q): Query<DocDataQuery>,
    headers: HeaderMap,
    body: Option<Json<Value>>,
) -> Result<axum::response::Response> {
    debug!(
        target: "cmx_doc::api",
        handler = "doc_data_sqlx_dataset_json",
        doc.module = %q.module.as_deref().unwrap_or("(auto)"),
        doc.file = %q.file.as_deref().unwrap_or("(auto)"),
        "handler invoked"
    );
    doc_load_entry(
        Driver::Sqlx,
        Exit::DatasetJson,
        q,
        headers,
        body.map(|b| b.0),
    )
    .await
}

/// 装载单据数据-tokio-zmc-msgpack
///
/// `GET|POST /api/doc/data/tokio-zmc-msgpack` —— tokio-postgres + ZmcDataSet 零拷贝 + 列式 msgpack 二进制出口。
/// GET 走便捷路径：URL query 传坐标（domain / application / module / file / doc）+
/// `filter=列名:值` 简单等值 + `limit` + `depth`；POST 时 body 为 DocQuery 富查询
/// JSON（URL 上的 depth / limit 作为兜底默认值）：
///
/// ```json
/// {
///   "depth": 2,
///   "includeSiblings": true,
///   "countTotal": false,
///   "layers": {
///     "<层 id>": {
///       "filter": { "status": "posted", "amount": { "$gt": 0 },
///                   "$or": [ { "type": "1" }, { "type": "2" } ] },
///       "orderBy": ["!posting_date", "code"],
///       "limit": 50,
///       "offset": 0,
///       "cursor": "<base64 游标>"
///     }
///   }
/// }
/// ```
///
/// filter 语义：键值对隐式 AND；标量值 = 等值简写；支持算子 `$eq` `$ne` `$gt` `$gte`
/// `$lt` `$lte` `$like` `$ilike` `$contains` `$startsWith` `$endsWith` `$null` 及
/// `$or` / `$and` 组合；`orderBy` 元素前缀 `!` 表示降序。
#[utoipa::path(
    post,
    path = "/api/doc/data/tokio-zmc-msgpack",
    params(DocDataQuery),
    request_body = Value,
    responses(
        (status = 200, description = "列式 msgpack 二进制信封：{code, msg, data}（data 为列式编码字节）", content_type = "application/x-msgpack")
    ),
    tag = "DOC单据接口"
)]
pub async fn doc_data_tokio_zmc_msgpack(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Query(q): Query<DocDataQuery>,
    headers: HeaderMap,
    body: Option<Json<Value>>,
) -> Result<axum::response::Response> {
    debug!(
        target: "cmx_doc::api",
        handler = "doc_data_tokio_zmc_msgpack",
        doc.module = %q.module.as_deref().unwrap_or("(auto)"),
        doc.file = %q.file.as_deref().unwrap_or("(auto)"),
        "handler invoked"
    );
    doc_load_entry(
        Driver::Tokio,
        Exit::ZmcMsgpack,
        q,
        headers,
        body.map(|b| b.0),
    )
    .await
}

/// 装载单据数据-sqlx-zmc-msgpack
///
/// `GET|POST /api/doc/data/sqlx-zmc-msgpack` —— sqlx + ZmcDataSet 零拷贝 + 列式 msgpack 二进制出口。
/// GET 走便捷路径：URL query 传坐标（domain / application / module / file / doc）+
/// `filter=列名:值` 简单等值 + `limit` + `depth`；POST 时 body 为 DocQuery 富查询
/// JSON（URL 上的 depth / limit 作为兜底默认值）：
///
/// ```json
/// {
///   "depth": 2,
///   "includeSiblings": true,
///   "countTotal": false,
///   "layers": {
///     "<层 id>": {
///       "filter": { "status": "posted", "amount": { "$gt": 0 },
///                   "$or": [ { "type": "1" }, { "type": "2" } ] },
///       "orderBy": ["!posting_date", "code"],
///       "limit": 50,
///       "offset": 0,
///       "cursor": "<base64 游标>"
///     }
///   }
/// }
/// ```
///
/// filter 语义：键值对隐式 AND；标量值 = 等值简写；支持算子 `$eq` `$ne` `$gt` `$gte`
/// `$lt` `$lte` `$like` `$ilike` `$contains` `$startsWith` `$endsWith` `$null` 及
/// `$or` / `$and` 组合；`orderBy` 元素前缀 `!` 表示降序。
#[utoipa::path(
    post,
    path = "/api/doc/data/sqlx-zmc-msgpack",
    params(DocDataQuery),
    request_body = Value,
    responses(
        (status = 200, description = "列式 msgpack 二进制信封：{code, msg, data}（data 为列式编码字节）", content_type = "application/x-msgpack")
    ),
    tag = "DOC单据接口"
)]
pub async fn doc_data_sqlx_zmc_msgpack(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Query(q): Query<DocDataQuery>,
    headers: HeaderMap,
    body: Option<Json<Value>>,
) -> Result<axum::response::Response> {
    debug!(
        target: "cmx_doc::api",
        handler = "doc_data_sqlx_zmc_msgpack",
        doc.module = %q.module.as_deref().unwrap_or("(auto)"),
        doc.file = %q.file.as_deref().unwrap_or("(auto)"),
        "handler invoked"
    );
    doc_load_entry(
        Driver::Sqlx,
        Exit::ZmcMsgpack,
        q,
        headers,
        body.map(|b| b.0),
    )
    .await
}

/// 装载单据数据-tokio-zmc-json
///
/// `GET|POST /api/doc/data/tokio-zmc-json` —— tokio-postgres + ZmcDataSet 零拷贝 + 纯 JSON 出口。
/// GET 走便捷路径：URL query 传坐标（domain / application / module / file / doc）+
/// `filter=列名:值` 简单等值 + `limit` + `depth`；POST 时 body 为 DocQuery 富查询
/// JSON（URL 上的 depth / limit 作为兜底默认值）：
///
/// ```json
/// {
///   "depth": 2,
///   "includeSiblings": true,
///   "countTotal": false,
///   "layers": {
///     "<层 id>": {
///       "filter": { "status": "posted", "amount": { "$gt": 0 },
///                   "$or": [ { "type": "1" }, { "type": "2" } ] },
///       "orderBy": ["!posting_date", "code"],
///       "limit": 50,
///       "offset": 0,
///       "cursor": "<base64 游标>"
///     }
///   }
/// }
/// ```
///
/// filter 语义：键值对隐式 AND；标量值 = 等值简写；支持算子 `$eq` `$ne` `$gt` `$gte`
/// `$lt` `$lte` `$like` `$ilike` `$contains` `$startsWith` `$endsWith` `$null` 及
/// `$or` / `$and` 组合；`orderBy` 元素前缀 `!` 表示降序。
#[utoipa::path(
    post,
    path = "/api/doc/data/tokio-zmc-json",
    params(DocDataQuery),
    request_body = Value,
    responses(
        (status = 200, description = "列式包（ApiResp JSON 信封）：datasetId / columns / rows / childRows，前端 CmxDataSet.fromJSON 直接装载", body = ApiResp<Value>)
    ),
    tag = "DOC单据接口"
)]
pub async fn doc_data_tokio_zmc_json(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Query(q): Query<DocDataQuery>,
    headers: HeaderMap,
    body: Option<Json<Value>>,
) -> Result<axum::response::Response> {
    debug!(
        target: "cmx_doc::api",
        handler = "doc_data_tokio_zmc_json",
        doc.module = %q.module.as_deref().unwrap_or("(auto)"),
        doc.file = %q.file.as_deref().unwrap_or("(auto)"),
        "handler invoked"
    );
    doc_load_entry(Driver::Tokio, Exit::ZmcJson, q, headers, body.map(|b| b.0)).await
}

/// 装载单据数据-sqlx-zmc-json
///
/// `GET|POST /api/doc/data/sqlx-zmc-json` —— sqlx + ZmcDataSet 零拷贝 + 纯 JSON 出口。
/// GET 走便捷路径：URL query 传坐标（domain / application / module / file / doc）+
/// `filter=列名:值` 简单等值 + `limit` + `depth`；POST 时 body 为 DocQuery 富查询
/// JSON（URL 上的 depth / limit 作为兜底默认值）：
///
/// ```json
/// {
///   "depth": 2,
///   "includeSiblings": true,
///   "countTotal": false,
///   "layers": {
///     "<层 id>": {
///       "filter": { "status": "posted", "amount": { "$gt": 0 },
///                   "$or": [ { "type": "1" }, { "type": "2" } ] },
///       "orderBy": ["!posting_date", "code"],
///       "limit": 50,
///       "offset": 0,
///       "cursor": "<base64 游标>"
///     }
///   }
/// }
/// ```
///
/// filter 语义：键值对隐式 AND；标量值 = 等值简写；支持算子 `$eq` `$ne` `$gt` `$gte`
/// `$lt` `$lte` `$like` `$ilike` `$contains` `$startsWith` `$endsWith` `$null` 及
/// `$or` / `$and` 组合；`orderBy` 元素前缀 `!` 表示降序。
#[utoipa::path(
    post,
    path = "/api/doc/data/sqlx-zmc-json",
    params(DocDataQuery),
    request_body = Value,
    responses(
        (status = 200, description = "列式包（ApiResp JSON 信封）：datasetId / columns / rows / childRows，前端 CmxDataSet.fromJSON 直接装载", body = ApiResp<Value>)
    ),
    tag = "DOC单据接口"
)]
pub async fn doc_data_sqlx_zmc_json(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Query(q): Query<DocDataQuery>,
    headers: HeaderMap,
    body: Option<Json<Value>>,
) -> Result<axum::response::Response> {
    debug!(
        target: "cmx_doc::api",
        handler = "doc_data_sqlx_zmc_json",
        doc.module = %q.module.as_deref().unwrap_or("(auto)"),
        doc.file = %q.file.as_deref().unwrap_or("(auto)"),
        "handler invoked"
    );
    doc_load_entry(Driver::Sqlx, Exit::ZmcJson, q, headers, body.map(|b| b.0)).await
}

// ── 懒下钻端点：只装某层在指定父下的子树 ─────────────────────────────────────

/// `POST /api/doc/data/children` 请求体。
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct DocChildrenReq {
    /// 域；可选：缺失时后端按 doc(moduleCode)/file 全局反查（多 DAM 冲突返回 409 + 候选列表）。
    #[serde(default)]
    pub domain: Option<String>,
    /// 应用；可选：同 `domain`，缺失时全局反查补全。
    #[serde(default)]
    pub application: Option<String>,
    /// 模块；可选：同 `domain`，缺失时全局反查补全。
    #[serde(default)]
    pub module: Option<String>,
    /// 单据定义文件名；缺失/空时由后端按坐标自动解析默认 DOC 文件（与装载接口一致）。
    #[serde(default)]
    pub file: Option<String>,
    /// 单据模块编码（moduleMeta.moduleCode）；前端走 code 定位时传，后端据此精确定义文件。
    #[serde(default)]
    pub doc: Option<String>,
    /// 要下钻装载的层 id。
    pub layer: String,
    /// 上层选中的父 id 列表（该层 childKey 匹配）。
    #[serde(rename = "parentIds")]
    pub parent_ids: Vec<Value>,
    /// 该层查询（filter/orderBy/limit/offset/cursor）。
    #[serde(default)]
    pub query: Option<Value>,
    /// 深度（从该层继续下钻几层；None=只装该层）。
    #[serde(default)]
    pub depth: Option<usize>,
    /// 出口通道（缺省 tokio-zmc-json）。可选 "sqlx-zmc-json"。
    #[serde(default)]
    pub exit: Option<String>,
}

/// 懒下钻装载子树。
///
/// `POST /api/doc/data/children` —— 装载某层在给定父 id 下的子树（含可选孙层）。
/// 前端 grid 展开某父行时调用。通用（元数据驱动）：`layer` 是任意层 id，childKey
/// 由元数据推导，该层查询由 body 的 `query` 指定（filter / orderBy / limit / offset /
/// cursor），全部经 `build_layer_select` 生成参数化 SQL。
#[utoipa::path(
    post,
    path = "/api/doc/data/children",
    request_body = DocChildrenReq,
    responses(
        (status = 200, description = "子树列式包（纯 JSON），前端 CmxDataSet.fromJSON 直接回填父行 _children", body = ApiResp<Value>)
    ),
    tag = "DOC单据接口"
)]
pub async fn doc_children(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(req): Json<DocChildrenReq>,
) -> Result<Json<ApiResp<Value>>> {
    use cmx_database_pg::get_default_pg_db_manager;
    use cmx_doc_store_pg::{ZmcDocLoader, ZmcDocLoaderSqlx};

    debug!(
        target: "cmx_doc::api",
        handler = "doc_children",
        doc.module = %req.module.as_deref().unwrap_or("(auto)"),
        doc.layer = %req.layer,
        "handler invoked"
    );
    let db_id = resolve_db_id_from_headers(&headers).await;
    let (meta, _file) = resolve_doc_meta(
        req.domain.as_deref(),
        req.application.as_deref(),
        req.module.as_deref(),
        req.file.as_deref(),
        req.doc.as_deref(),
    )
    .await?;

    // 组一个 DocQuery：把该层的查询塞进去 + depth。
    let mut dq = DocQuery {
        include_siblings: true,
        depth: req.depth,
        ..Default::default()
    };
    if let Some(v) = &req.query
        && !v.is_null()
    {
        let lq_json = serde_json::json!({ "layers": { &req.layer: v } });
        let parsed = DocQuery::from_json(&lq_json)?;
        if let Some(lq) = parsed.layers.get(&req.layer) {
            dq.layers.insert(req.layer.clone(), lq.clone());
        }
    }
    dq.validate(&meta)?;

    // 以该层为根、给定父 id 下钻装载子树（sqlx 可选，默认 tokio）。
    let use_sqlx = req.exit.as_deref() == Some("sqlx-zmc-json");
    let pkg = if use_sqlx {
        let mm = get_default_db_manager();
        let zmc =
            ZmcDocLoaderSqlx::load_subtree(mm, &db_id, &meta, &req.layer, &req.parent_ids, &dq)
                .await?;
        zmc.encode_columnar_json()
    } else {
        let mm = get_default_pg_db_manager();
        let zmc =
            ZmcDocLoader::load_subtree(mm, &db_id, &meta, &req.layer, &req.parent_ids, &dq).await?;
        zmc.encode_columnar_json()
    };

    Ok(Json(ApiResp::ok(pkg)))
}

// ── 真·流式端点：超大扁平结果零内存 chunked 传输 ──────────────────────────────

/// 流式装载单层大结果。
///
/// `GET|POST /api/doc/data/tokio-zmc-stream` —— 长度分帧二进制流。
/// 面向超大扁平单层结果（不下钻子层）：服务端逐行流式吐出，峰值内存 O(单行)。
/// 响应为长度分帧二进制流（`Content-Type: application/octet-stream`、chunked），
/// 前端用 cmx-msgpack-stream 解码。
///
/// GET 走便捷路径：URL query 传坐标 + `filter=列名:值` + `limit`；POST body（单层查询）：
///
/// ```json
/// {
///   "layer": "<目标层 id，缺省根层>",
///   "filter": { "status": "posted" },
///   "orderBy": ["code"],
///   "limit": 100000
/// }
/// ```
#[utoipa::path(
    post,
    path = "/api/doc/data/tokio-zmc-stream",
    params(DocDataQuery),
    request_body = Value,
    responses(
        (status = 200, description = "长度分帧二进制流（chunked 传输），前端 cmx-msgpack-stream 解码", content_type = "application/octet-stream")
    ),
    tag = "DOC单据接口"
)]
pub async fn doc_data_stream(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Query(q): Query<DocDataQuery>,
    headers: HeaderMap,
    body: Option<Json<Value>>,
) -> Result<axum::response::Response> {
    use axum::response::IntoResponse;
    use cmx_database_pg::get_default_pg_db_manager;
    use cmx_doc_store_pg::{LayerQuery, build_layer_select};

    debug!(
        target: "cmx_doc::api",
        handler = "doc_data_stream",
        doc.module = %q.module.as_deref().unwrap_or("(auto)"),
        doc.file = %q.file.as_deref().unwrap_or("(auto)"),
        "handler invoked"
    );
    let db_id = resolve_db_id_from_headers(&headers).await;
    let (meta, _file) = resolve_doc_meta(
        q.domain.as_deref(),
        q.application.as_deref(),
        q.module.as_deref(),
        q.file.as_deref(),
        q.doc.as_deref(),
    )
    .await?;

    // 目标层：body.layer 指定，否则根层。流式**只装该单层**（扁平大结果，不嵌套）。
    let body_val = body.map(|b| b.0).unwrap_or(Value::Null);
    let layer_id = body_val
        .get("layer")
        .and_then(|v| v.as_str())
        .map(String::from)
        .or_else(|| meta.root_layer().map(|l| l.id.clone()))
        .ok_or_else(|| cmx_biz::BizError::business("单据无根层"))?;
    let layer = meta
        .layer(&layer_id)
        .ok_or_else(|| cmx_biz::BizError::business(format!("层 {layer_id} 不在定义中")))?;

    // 该层查询：body 有 filter/orderBy/limit 则用之，否则 GET 便捷 filter/limit。
    let lq = if body_val.is_object() {
        let dq = DocQuery::from_json(&serde_json::json!({ "layers": { &layer_id: body_val } }))?;
        dq.layer(&layer_id)
    } else {
        let filter = if let Some(f) = &q.filter
            && let Some((col, val)) = f.split_once(':')
        {
            cmx_doc_store_pg::Filter::from_json(&serde_json::json!({ col: val }))?
        } else {
            None
        };
        LayerQuery {
            limit: q.limit,
            filter,
            ..Default::default()
        }
    };
    lq.validate_against(layer)?;

    // 生成参数化 SQL（无 parent_scope：单层根查询）
    let (sql, params) = build_layer_select(layer, &lq, None)?;
    let dataset_id = layer.id.clone();
    // header 帧列名（与 SELECT 列顺序一致 = 定义 schema 字段顺序）——先于结果流发出，空结果也收尾。
    let col_names: Vec<String> = layer.schema.fields.iter().map(|f| f.name.clone()).collect();

    // 背压 channel：容量有限，producer 满则 await（受下游网络速度节流 → 内存平稳）。
    let (tx, rx) = tokio::sync::mpsc::channel::<bytes::Bytes>(8);

    // producer：独占一条连接跑流式查询，逐帧发到 channel。db_id/sql/params move 进 task。
    tokio::spawn(async move {
        let mm = get_default_pg_db_manager();
        if let Err(e) = mm
            .query_sql_zmc_stream_chunks(&db_id, &sql, params, &dataset_id, col_names, tx)
            .await
        {
            tracing::warn!(
                target: "cmx_doc::load",
                dataset_id = %dataset_id,
                error = %e,
                "stream load failed"
            );
        }
    });

    // Body::from_stream over receiver：axum 逐块 flush 给客户端（chunked）。
    let stream = futures::stream::unfold(rx, |mut rx| async move {
        rx.recv()
            .await
            .map(|chunk| (Ok::<bytes::Bytes, std::io::Error>(chunk), rx))
    });
    let response_body = axum::body::Body::from_stream(stream);

    Ok((
        [
            (axum::http::header::CONTENT_TYPE, "application/octet-stream"),
            (axum::http::header::CACHE_CONTROL, "no-cache"),
        ],
        response_body,
    )
        .into_response())
}

/// 返回单据显示元数据。
///
/// `GET /api/doc/meta` —— 内容为层序 L1..LN + 各层列 caption / 类型 + 父子关系。
/// 数据包（`/api/doc/data*`）
/// 只带列名，不带 caption / 类型 / 宽度；通用单据前端页据此端点动态构建 N 层主从
/// schema、各层 grid 与列头。复用已解析 + 缓存 + 合并 base 字段集的 `DocMetaView`
/// （与装载器同一真相源），投影成前端友好 JSON。参数同 [`DocDataQuery`] 的坐标部分。
#[utoipa::path(
    get,
    path = "/api/doc/meta",
    params(DocDataQuery),
    responses(
        (status = 200, description = "单据显示元数据：层序 L1..LN + 各层列 caption / 类型 + 父子关系", body = ApiResp<Value>)
    ),
    tag = "DOC单据接口"
)]
pub async fn doc_meta(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Query(q): Query<DocDataQuery>,
    headers: HeaderMap,
) -> Result<Json<ApiResp<Value>>> {
    let _ = &headers; // meta 与 db_id 无关(定义读取不走数据源);保留签名一致
    debug!(
        target: "cmx_doc::api",
        handler = "doc_meta",
        doc.module = %q.module.as_deref().unwrap_or("(auto)"),
        doc.file = %q.file.as_deref().unwrap_or("(auto)"),
        "handler invoked"
    );

    let (meta, _file) = resolve_doc_meta(
        q.domain.as_deref(),
        q.application.as_deref(),
        q.module.as_deref(),
        q.file.as_deref(),
        q.doc.as_deref(),
    )
    .await?;
    Ok(Json(ApiResp::ok(cmx_doc_model::project_doc_meta(&meta))))
}

/// 构造成功信封的 msgpack 字节:`{code:0, msg:"success", data:<已编码的 data 字节>}`。
// encode_envelope_ok / msgpack_response 已上提到 cmx_api_core::msgpack（与 dct 共用）。
use cmx_api_core::msgpack::msgpack_ok_response;

/// `POST /api/doc/save` / `POST /api/doc/restore` 共用的单据坐标 query 参数。
#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct DocSaveQuery {
    /// 域；可选：缺失时后端按 doc(moduleCode)/file 全局反查（多 DAM 冲突返回 409 + 候选列表）。
    #[serde(default)]
    pub domain: Option<String>,
    /// 应用；可选：同 `domain`，缺失时全局反查补全。
    #[serde(default)]
    pub application: Option<String>,
    /// 模块；可选：同 `domain`，缺失时全局反查补全。
    #[serde(default)]
    pub module: Option<String>,
    /// 单据定义文件名；缺失/空时由后端按坐标自动解析默认 DOC 文件（与装载接口一致）。
    #[serde(default)]
    pub file: Option<String>,
    /// 单据模块编码（moduleMeta.moduleCode）；前端走 code 定位时传，后端据此精确定义文件。
    #[serde(default)]
    pub doc: Option<String>,
}

/// 回存单据数据。
///
/// `POST /api/doc/save` —— merge/replace 双模式。
/// 单据坐标走 query 参数（[`DocSaveQuery`]），body 为 changeset JSON：
///
/// ```json
/// {
///   "saveMode": "merge",
///   "changes": {
///     "<层 id>": {
///       "inserted": [ { "id": "临时 id", "upper_id": "父行 id", "fields": { "...": "..." } } ],
///       "updated": [ { "id": "...", "fields": { "...": "..." }, "baseline": { "update_time": "..." } } ],
///       "deleted": [ "id1", "id2" ]
///     }
///   }
/// }
/// ```
///
/// `saveMode=replace` 时改用 `snapshot` 整树替换。保存自动带审计填充
/// （create_by / update_by）、版本快照与乐观锁；列级校验 / validationRules 失败时
/// 返回结构化 violations（HTTP 200 错误信封），前端逐行逐列高亮。
#[utoipa::path(
    post,
    path = "/api/doc/save",
    params(DocSaveQuery),
    request_body = Value,
    responses(
        (status = 200, description = "保存结果（affected / idMap 等）；列级校验失败返回结构化 violations", body = ApiResp<Value>)
    ),
    tag = "DOC单据接口"
)]
pub async fn doc_save(
    State(_s): State<CmxAppState>,
    ctx: CmxSvrContext,
    Query(q): Query<DocSaveQuery>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<ApiResp<Value>>> {
    debug!(
        target: "cmx_doc::api",
        handler = "doc_save",
        doc.module = %q.module.as_deref().unwrap_or("(auto)"),
        doc.file = ?q.file,
        "handler invoked"
    );
    let mm = get_default_db_manager();
    let db_id = resolve_db_id_from_headers(&headers).await;

    // 前端走 doc(moduleCode) 定位，file 由 resolve_doc_meta 内部兜底（对齐 dct_save 的 resolve_dict）。
    let (meta, file) = resolve_doc_meta(
        q.domain.as_deref(),
        q.application.as_deref(),
        q.module.as_deref(),
        q.file.as_deref(),
        q.doc.as_deref(),
    )
    .await?;
    let (mode, changes) = saver::parse_save_body(&body);

    // §14.2 后端二次校验：对 changeset 各行跑 validationRules，error 阻断保存。
    if !meta.validation_rules.is_empty()
        && let Some(vr) = run_validation(&meta, &changes)
    {
        return Ok(Json(ApiResp::ok(vr)));
    }

    let result = match DocSaver::save(
        mm,
        &db_id,
        &meta,
        mode,
        &changes,
        &save_ctx(&ctx, &file, None),
    )
    .await
    {
        Ok(r) => r,
        // 列级校验失败：返回结构化 422（data.violations），前端逐行逐列高亮。
        Err(e) => {
            if let Some(vs) = e.violations() {
                return Ok(Json(validation_fail_resp(vs)));
            }
            return Err(e.into());
        }
    };

    Ok(Json(ApiResp::ok(
        serde_json::to_value(result).map_err(serde_err_to_api)?,
    )))
}

/// 从 `CmxSvrContext` 构造保存上下文（审计填充 方案 C + 版本快照 B1）。
///
/// - `actor_id`：`create_by`/`update_by` 是 BIGINT，而 `auth_context.user_id` 是 String（系统身份为
///   字面量 "system"）。缺失/空/非数字 → 兜底 `0`（约定 0=系统），保存**永不因身份缺失失败**。
/// - `actor_name`：版本台账 actor_name；空则 "系统"（对齐 `handler.rs` 的 `model_operator` 兜底惯例）。
/// - `doc_file`：单据定义文件名，版本台账定位「哪种单据」。
/// - `op_override`：restore 等传 Some("restore")；None 时 saver 按 changeset 桶推断 create/update。
fn save_ctx(
    ctx: &CmxSvrContext,
    doc_file: &str,
    op_override: Option<&str>,
) -> cmx_doc_store_pg::SaveCtx {
    let actor_id = actor_id_i64(ctx);
    let actor_name = actor_name(ctx);
    cmx_doc_store_pg::SaveCtx {
        actor_id,
        actor_name,
        doc_file: doc_file.to_string(),
        op_override: op_override.map(String::from),
    }
}

/// 批量回存单据。
///
/// `POST /api/doc/save/batch` —— 一批可混多种单据（每单自带坐标），body：
///
/// ```json
/// {
///   "atomic": true,
///   "docs": [
///     {
///       "domain": "fi", "application": "cmxfico", "module": "gl",
///       "file": "cmxfico_doc_meta_v1.json", "doc": "cmxfico",
///       "saveMode": "merge",
///       "changes": { "<层 id>": { "inserted": [], "updated": [], "deleted": [] } }
///     }
///   ]
/// }
/// ```
///
/// `atomic` 缺省 true：一个大事务全成全败；false 时每单独立事务、逐单成败。
/// 每单自动享审计填充 / 版本快照 / 乐观锁；单单 changeset 格式同 `/api/doc/save`。
#[utoipa::path(
    post,
    path = "/api/doc/save/batch",
    request_body = Value,
    responses(
        (status = 200, description = "批量保存结果 {atomic, count, results}；校验失败返回 {atomic, failedIndex, validation}", body = ApiResp<Value>)
    ),
    tag = "DOC单据接口"
)]
pub async fn doc_save_batch(
    State(_s): State<CmxAppState>,
    ctx: CmxSvrContext,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_db_manager();
    let db_id = resolve_db_id_from_headers(&headers).await;

    let atomic = body.get("atomic").and_then(|v| v.as_bool()).unwrap_or(true);
    let docs = body
        .get("docs")
        .and_then(|v| v.as_array())
        .ok_or_else(|| cmx_biz::BizError::business("批量保存缺少 docs 数组"))?;
    if docs.is_empty() {
        return Ok(Json(ApiResp::ok(
            serde_json::json!({ "atomic": atomic, "results": [] }),
        )));
    }

    // 逐单解析：resolve meta（缓存）+ parse_save_body + 校验 + save_ctx。
    // 先落地各单的 owned 数据（meta/changes/sctx），再借用构造 BatchItem（借用检查要求）。
    let mut metas: Vec<std::sync::Arc<DocMetaView>> = Vec::with_capacity(docs.len());
    let mut parsed: Vec<(saver::SaveMode, Value)> = Vec::with_capacity(docs.len());
    let mut ctxs: Vec<cmx_doc_store_pg::SaveCtx> = Vec::with_capacity(docs.len());
    for (i, d) in docs.iter().enumerate() {
        let get = |k: &str| d.get(k).and_then(|v| v.as_str()).unwrap_or("");
        // DAM 缺失时按 doc(moduleCode)/file 全局反查补全（同单单 /doc/save 路径，空串归一为 None）。
        let dom = (!get("domain").is_empty()).then_some(get("domain"));
        let app = (!get("application").is_empty()).then_some(get("application"));
        let mdl = (!get("module").is_empty()).then_some(get("module"));
        let raw_file = get("file");
        let raw_doc = get("doc");
        let f = (!raw_file.is_empty()).then_some(raw_file);
        let dc = (!raw_doc.is_empty()).then_some(raw_doc);
        let (meta, file) = resolve_doc_meta(dom, app, mdl, f, dc).await?;
        let (mode, changes) = saver::parse_save_body(d);
        // 后端二次校验（同单单路径）：有 error 违规即整批拒（atomic）/该单在 save 阶段无从表达，故这里统一先拒。
        if !meta.validation_rules.is_empty()
            && let Some(vr) = run_validation(&meta, &changes)
        {
            return Ok(Json(ApiResp::ok(serde_json::json!({
                "atomic": atomic,
                "failedIndex": i,
                "validation": vr,
            }))));
        }
        ctxs.push(save_ctx(&ctx, &file, None));
        metas.push(meta);
        parsed.push((mode, changes));
    }

    let items: Vec<cmx_doc_store_pg::BatchItem> = (0..docs.len())
        .map(|i| cmx_doc_store_pg::BatchItem {
            meta: &metas[i],
            mode: parsed[i].0,
            changes: &parsed[i].1,
            sctx: &ctxs[i],
        })
        .collect();

    let results = DocSaver::save_batch(mm, &db_id, &items, atomic).await?;
    Ok(Json(ApiResp::ok(serde_json::json!({
        "atomic": atomic,
        "count": results.len(),
        "results": serde_json::to_value(&results).map_err(serde_err_to_api)?,
    }))))
}

/// 对 changeset 各层各行跑校验规则。返回 Some(错误响应) 表示有 error 违规（阻断）；None 表示通过。
fn run_validation(meta: &DocMetaView, changes: &Value) -> Option<Value> {
    let obj = changes.as_object()?;
    let mut all_violations = Vec::new();
    for (_layer, layer_changes) in obj {
        // 校验 inserted + updated 行（deleted 无需校验）
        for bucket in ["inserted", "updated"] {
            if let Some(rows) = layer_changes.get(bucket).and_then(|v| v.as_array()) {
                for row in rows {
                    // 行 scope = 顶层 id/upper_id + fields 铺平
                    let scope = build_row_scope(row);
                    let res = cmx_doc_store_pg::validate(&meta.validation_rules, &scope);
                    for v in res.violations {
                        if v.severity == "error" {
                            all_violations.push(serde_json::json!({
                                "code": v.code, "message": v.message, "level": v.level,
                            }));
                        }
                    }
                }
            }
        }
    }
    if all_violations.is_empty() {
        None
    } else {
        Some(serde_json::json!({
            "ok": false,
            "errorCode": "DOC_VALIDATION_FAILED",
            "violations": all_violations,
        }))
    }
}

/// 从 changeset 行 { id, upper_id, fields:{...} } 铺平成求值 scope。
fn build_row_scope(row: &Value) -> cmx_doc_store_pg::Scope {
    let mut flat = serde_json::Map::new();
    for top in ["id", "upper_id", "line_no"] {
        if let Some(v) = row.get(top) {
            flat.insert(top.to_string(), v.clone());
        }
    }
    if let Some(fields) = row.get("fields").and_then(|v| v.as_object()) {
        for (k, v) in fields {
            flat.insert(k.clone(), v.clone());
        }
    }
    cmx_doc_store_pg::scope_from_json(&Value::Object(flat))
}

/// serde_json 序列化错误 → api Error（局部小工具）。
fn serde_err_to_api(e: serde_json::Error) -> cmx_api_core::Error {
    cmx_biz::BizError::internal(format!("序列化保存结果失败: {e}")).into()
}

// ─────────────────── 版本化查询/回滚（方案 §6A.5，Phase 8）───────────────────

/// `GET /api/doc/revisions` / `GET /api/doc/revision` 共用的版本查询参数。
#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct RevisionsQuery {
    /// 单据定义文件名（版本台账定位「哪种单据」）。
    #[serde(rename = "docFile")]
    pub doc_file: String,
    /// 单据根行 id。
    #[serde(rename = "rootId")]
    pub root_id: String,
    /// 目标版本号；缺省取当前版（is_current=1）。revision 端点取快照时用。
    pub rev: Option<i32>,
}

/// 列出版本时间线。
///
/// `GET /api/doc/revisions` —— 按 `docFile` + `rootId` 查询版本台账，返回该单据历次
/// 保存的版本列表（rev_no / is_current / op / reason / actor_name / created_at 等，
/// 按 rev 降序），供版本对比与回滚选择。
#[utoipa::path(
    get,
    path = "/api/doc/revisions",
    params(RevisionsQuery),
    responses(
        (status = 200, description = "版本时间线：该单据根行的全部历史版本", body = ApiResp<Value>)
    ),
    tag = "DOC单据接口"
)]
pub async fn doc_revisions(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Query(q): Query<RevisionsQuery>,
    headers: HeaderMap,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_db_manager();
    let db_id = resolve_db_id_from_headers(&headers).await;
    let list = DocRevision::list(mm, &db_id, &q.doc_file, &q.root_id).await?;
    Ok(Json(ApiResp::ok(list)))
}

/// 取历史版本快照。
///
/// `GET /api/doc/revision` —— 按 `docFile` + `rootId` + `rev` 返回该版本的列式包快照
/// （结构与装载接口数据包一致），前端 `CmxDataSet.fromJSON` 直接渲染。`rev` 缺省取
/// 当前版（is_current=1）。
#[utoipa::path(
    get,
    path = "/api/doc/revision",
    params(RevisionsQuery),
    responses(
        (status = 200, description = "该版本的完整快照（列式包，前端 CmxDataSet.fromJSON 直接渲染）", body = ApiResp<Value>)
    ),
    tag = "DOC单据接口"
)]
pub async fn doc_revision(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Query(q): Query<RevisionsQuery>,
    headers: HeaderMap,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_db_manager();
    let db_id = resolve_db_id_from_headers(&headers).await;
    let snap = DocRevision::get_snapshot(mm, &db_id, &q.doc_file, &q.root_id, q.rev).await?;
    Ok(Json(ApiResp::ok(snap)))
}

/// 恢复历史版本。
///
/// `POST /api/doc/restore` —— 取该版快照后以 replace 模式写回（op=restore），历史
/// 版本不丢失。单据坐标走 query 参数（[`DocSaveQuery`]），body：
///
/// ```json
/// { "rootId": "<根行 id>", "rev": 3 }
/// ```
///
/// 取该版快照后以 replace 模式写回（op=restore），历史版本不丢失。`rev` 缺省时
/// 使用当前版（is_current=1）快照。
#[utoipa::path(
    post,
    path = "/api/doc/restore",
    params(DocSaveQuery),
    request_body = Value,
    responses(
        (status = 200, description = "恢复结果 {ok, mode: restore, affected, restoredRev}", body = ApiResp<Value>)
    ),
    tag = "DOC单据接口"
)]
pub async fn doc_restore(
    State(_s): State<CmxAppState>,
    ctx: CmxSvrContext,
    Query(q): Query<DocSaveQuery>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_db_manager();
    let db_id = resolve_db_id_from_headers(&headers).await;

    let root_id = body
        .get("rootId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| cmx_biz::BizError::business("restore 缺少 rootId"))?;
    let rev = body.get("rev").and_then(|v| v.as_i64()).map(|n| n as i32);

    // 前端走 doc(moduleCode) 定位，file 由 resolve_doc_meta 内部兜底（同 doc_save）。
    let (meta, file) = resolve_doc_meta(
        q.domain.as_deref(),
        q.application.as_deref(),
        q.module.as_deref(),
        q.file.as_deref(),
        q.doc.as_deref(),
    )
    .await?;

    // 取历史版快照（列式包）
    let snapshot = DocRevision::get_snapshot(mm, &db_id, &file, root_id, rev).await?;
    if snapshot.is_null() {
        return Err(cmx_biz::BizError::not_found("指定版本不存在").into());
    }

    // 用 replace 模式把快照写回（DocSaver 内部单事务）
    // 快照是列式包 { datasetId, columns, rows, childRows }；replace 期望 { table:{rows:[{id,upper_id,fields}]} }
    // 这里把列式包转成 replace 输入（简化：交给 DocSaver 前先归一）
    let replace_input = columnar_to_replace_input(&snapshot);
    let result = DocSaver::save(
        mm,
        &db_id,
        &meta,
        cmx_doc_store_pg::SaveMode::Replace,
        &replace_input,
        &save_ctx(&ctx, &file, Some("restore")),
    )
    .await?;

    Ok(Json(ApiResp::ok(serde_json::json!({
        "ok": result.ok,
        "mode": "restore",
        "affected": result.affected,
        "restoredRev": rev,
    }))))
}

/// 列式包 { datasetId, columns, rows:[[..]], childRows } → replace 输入
/// { table: { rows: [ {id, upper_id, fields:{..}} ] }, ... }（各层递归展平）。
fn columnar_to_replace_input(pkg: &Value) -> Value {
    let mut out = serde_json::Map::new();
    flatten_columnar(pkg, &mut out);
    Value::Object(out)
}

fn flatten_columnar(pkg: &Value, out: &mut serde_json::Map<String, Value>) {
    let Some(table) = pkg.get("datasetId").and_then(|v| v.as_str()) else {
        return;
    };
    let cols: Vec<String> = pkg
        .get("columns")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|c| c.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let empty = vec![];
    let rows = pkg.get("rows").and_then(|v| v.as_array()).unwrap_or(&empty);

    let mut out_rows = Vec::new();
    for row in rows {
        let vals = row.as_array().cloned().unwrap_or_default();
        let mut obj = serde_json::Map::new();
        let mut fields = serde_json::Map::new();
        for (i, col) in cols.iter().enumerate() {
            let v = vals.get(i).cloned().unwrap_or(Value::Null);
            match col.as_str() {
                "id" | "upper_id" | "line_no" => {
                    obj.insert(col.clone(), v);
                }
                _ => {
                    fields.insert(col.clone(), v);
                }
            }
        }
        obj.insert("fields".into(), Value::Object(fields));
        out_rows.push(Value::Object(obj));
    }
    out.insert(table.to_string(), serde_json::json!({ "rows": out_rows }));

    // 递归子层
    if let Some(child_rows) = pkg.get("childRows").and_then(|v| v.as_object()) {
        for per_child in child_rows.values() {
            if let Some(map) = per_child.as_object() {
                for child_pkg in map.values() {
                    flatten_columnar(child_pkg, out);
                }
            }
        }
    }
}

// DOC 定义文件解析（resolve_doc_file / doc_matches）+ 文件解析缓存已抽到
// cmx-model-meta::definitions::resolve（DOC/DCT 共享，供 cmx-api 的业务编码定位复用，避免
// cmx-api ⇄ cmx-doc 环）。读定义链路（resolve_doc_file_smart / resolve_doc_meta / load_base）
// 已下沉至 cmx-doc-store-pg::resolve（对齐 dct 的 resolve_dict 在 store-pg 层），本文件经 use 引用。
use cmx_doc_store_pg::resolve_doc_meta;

