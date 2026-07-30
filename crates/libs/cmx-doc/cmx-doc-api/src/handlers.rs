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

use std::sync::Arc;

use axum::Json;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use serde::Deserialize;
use serde_json::Value;
use tracing::debug;

use cmx_core::model::data::dataset::ColumnarCodec;
use cmx_database::get_default_db_manager;
use cmx_doc_store_pg::{DocLoader, DocMetaView, DocQuery, DocRevision, DocSaver, cache, saver};

use cmx_api::CmxAppState;
use cmx_api::middleware::CmxSvrContext;
use cmx_api::rest::header_parse::get_db_id_from_header;
use cmx_api::{ApiResp, Result};

/// `/api/doc/data/*` 装载端点共用查询参数（GET 便捷路径：URL query）。
#[derive(Debug, Deserialize)]
pub struct DocDataQuery {
    pub domain: String,
    pub application: String,
    pub module: String,
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
            Ok(msgpack_response(&buf))
        }
        // sqlx + Zmc + msgpack
        (Driver::Sqlx, Exit::ZmcMsgpack) => {
            let mm = get_default_db_manager();
            let zmc = ZmcDocLoaderSqlx::load(mm, db_id, meta, dq).await?;
            let mut buf = Vec::new();
            zmc.encode_columnar_binary(&mut buf);
            Ok(msgpack_response(&buf))
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

fn msgpack_response(columnar: &[u8]) -> axum::response::Response {
    use axum::response::IntoResponse;
    let body = encode_envelope_ok(columnar);
    (
        [(axum::http::header::CONTENT_TYPE, "application/x-msgpack")],
        body,
    )
        .into_response()
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
    let db_id = get_db_id_from_header(&headers).await;
    let meta = resolve_doc_meta(
        &q.domain,
        &q.application,
        &q.module,
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

/// `GET|POST /api/doc/data/sqlx-dataset-json` —— sqlx + 老 DataSet + JSON。
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
        doc.module = %q.module,
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

/// `GET|POST /api/doc/data/tokio-zmc-msgpack` —— tokio + ZmcDataSet + msgpack 二进制。
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
        doc.module = %q.module,
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

/// `GET|POST /api/doc/data/sqlx-zmc-msgpack` —— sqlx + ZmcDataSet + msgpack 二进制。
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
        doc.module = %q.module,
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

/// `GET|POST /api/doc/data/tokio-zmc-json` —— tokio + ZmcDataSet + 纯 JSON。
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
        doc.module = %q.module,
        doc.file = %q.file.as_deref().unwrap_or("(auto)"),
        "handler invoked"
    );
    doc_load_entry(Driver::Tokio, Exit::ZmcJson, q, headers, body.map(|b| b.0)).await
}

/// `GET|POST /api/doc/data/sqlx-zmc-json` —— sqlx + ZmcDataSet + 纯 JSON。
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
        doc.module = %q.module,
        doc.file = %q.file.as_deref().unwrap_or("(auto)"),
        "handler invoked"
    );
    doc_load_entry(Driver::Sqlx, Exit::ZmcJson, q, headers, body.map(|b| b.0)).await
}

// ── 懒下钻端点：只装某层在指定父下的子树 ─────────────────────────────────────

/// `POST /api/doc/data/children` 请求体。
#[derive(Debug, Deserialize)]
pub struct DocChildrenReq {
    pub domain: String,
    pub application: String,
    pub module: String,
    /// 单据定义文件名；缺失/空时由后端按坐标自动解析默认 DOC 文件（与装载接口一致）。
    #[serde(default)]
    pub file: Option<String>,
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

/// `POST /api/doc/data/children` —— 懒下钻：装载某层在给定父 id 下的子树（含可选孙层）。
///
/// 通用（元数据驱动）：`layer` 是任意层 id，childKey 由元数据推导，该层查询由 body.query
/// 指定，全部经 `build_layer_select`。前端 grid 展开某父行时调用。出口纯 JSON 列式包
/// （子树可直接 `CmxDataSet.fromJSON` 回填父行 `_children`）。
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
        doc.module = %req.module,
        doc.layer = %req.layer,
        "handler invoked"
    );
    let db_id = get_db_id_from_header(&headers).await;
    let meta = resolve_doc_meta(
        &req.domain,
        &req.application,
        &req.module,
        req.file.as_deref(),
        None,
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

/// `GET|POST /api/doc/data/tokio-zmc-stream` 请求参数（**单层扁平**大结果，不下钻）。
///
/// GET：URL query（domain/app/module/file + 便捷 filter/limit）；
/// POST：body = `{ layer?, filter?, orderBy?, limit? }`（不指定 layer 则用根层）。
/// 出口是**长度分帧**二进制流（`Content-Type: application/octet-stream`，
/// `Transfer-Encoding: chunked`），服务端峰值内存 O(单行)。前端用 cmx-msgpack-stream 解码。
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
        doc.module = %q.module,
        doc.file = %q.file.as_deref().unwrap_or("(auto)"),
        "handler invoked"
    );
    let db_id = get_db_id_from_header(&headers).await;
    let meta = resolve_doc_meta(
        &q.domain,
        &q.application,
        &q.module,
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

/// `GET /api/doc/meta` —— 返回单据**显示元数据**(层序 L1..LN + 各层列 caption/类型 + 父子关系)。
///
/// 数据包(`/api/doc/data*`)只带列名,不带 caption/类型/宽度;通用单据前端页据此端点**动态**
/// 构建 N 层主从 schema、各层 grid 与列头。复用已解析+缓存+合并 base 字段集的 `DocMetaView`
/// (与装载器同一真相源),投影成前端友好 JSON。参数同 [`DocDataQuery`] 的 domain/app/module/file。
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
        doc.module = %q.module,
        doc.file = %q.file.as_deref().unwrap_or("(auto)"),
        "handler invoked"
    );

    let meta = resolve_doc_meta(
        &q.domain,
        &q.application,
        &q.module,
        q.file.as_deref(),
        q.doc.as_deref(),
    )
    .await?;
    Ok(Json(ApiResp::ok(project_doc_meta(&meta))))
}

/// 把强类型 `DocMetaView` 投影成前端通用页要用的 JSON。
///
/// `layers` 输出**全部表**(含同层并列表,如 L4 的 cv_aux_line + cv_cyzb_line),每层带
/// `id/tableName/level/levelName/columns/summaries/aggFields`;每列带
/// `name/caption/dataType/dimType/agg/nullable/isPrimaryKey`;`summaries` 是本表汇总表(sum 表)。
/// 附 `layerGroups`(同层全部表分组) + `relations`(父子键) + `layerOrder`(主链路)。
fn project_doc_meta(meta: &DocMetaView) -> Value {
    // layers：输出全部表(不止主链路)——前端据 layerGroups/level 自行归组
    let layers: Vec<Value> = meta
        .layers
        .iter()
        .map(|l| {
            let cols: Vec<Value> = l.columns.iter().map(column_to_json).collect();
            let summaries: Vec<Value> = l
                .summaries
                .iter()
                .map(|s| {
                    let scols: Vec<Value> = s.columns.iter().map(column_to_json).collect();
                    serde_json::json!({
                        "id": s.id,
                        "name": s.name,
                        "caption": s.caption,
                        "sourceTable": s.source_table,
                        "columns": scols,
                    })
                })
                .collect();
            serde_json::json!({
                "id": l.id,
                "tableName": l.table_name,
                "level": l.level,
                "levelName": l.level_name,
                "columns": cols,
                "summaries": summaries,
                "aggFields": l.agg_fields,
            })
        })
        .collect();

    let layer_groups: Vec<Value> = meta
        .layer_groups
        .iter()
        .map(|g| {
            serde_json::json!({
                "level": g.level,
                "levelName": g.level_name,
                "tableIds": g.table_ids,
            })
        })
        .collect();

    let relations: Vec<Value> = meta
        .relations
        .iter()
        .map(|r| {
            serde_json::json!({
                "parent": r.parent,
                "child": r.child,
                "parentKey": r.parent_key,
                "childKey": r.child_key,
            })
        })
        .collect();

    serde_json::json!({
        "moduleCode": meta.module_code,
        "version": meta.version,
        "layerOrder": meta.layer_order,
        "layers": layers,
        "layerGroups": layer_groups,
        "relations": relations,
    })
}

/// 单列 → 前端 JSON（层列与汇总表列共用）。
fn column_to_json(c: &cmx_doc_store_pg::ColumnView) -> Value {
    let mut obj = serde_json::json!({
        "name": c.name,
        "caption": c.caption,
        "dataType": c.data_type,
        "dimType": c.dim_type,
        "agg": c.agg,
        "nullable": c.nullable,
        "isPrimaryKey": c.is_primary_key,
    });
    // 字典/录入控件配置：有值才输出，避免前端列对象携带大量空键。
    if !c.ref_dict.is_empty() {
        obj["refDict"] = serde_json::Value::String(c.ref_dict.clone());
    }
    if !c.display_field.is_empty() {
        obj["displayField"] = serde_json::Value::String(c.display_field.clone());
    }
    if !c.ref_field.is_empty() {
        obj["refField"] = serde_json::Value::String(c.ref_field.clone());
    }
    if let Some(edit) = &c.edit {
        obj["edit"] = edit.clone();
    }
    if let Some(es) = &c.edit_settings {
        obj["editSettings"] = es.clone();
    }
    // 显示属性（表现交互层）：有值才输出，供前端动态列模型格式化/对齐/显示精度。
    if let Some(d) = &c.display {
        obj["display"] = d.clone();
    }
    obj
}

/// 构造成功信封的 msgpack 字节:`{code:0, msg:"success", data:<已编码的 data 字节>}`。
fn encode_envelope_ok(data_msgpack: &[u8]) -> Vec<u8> {
    use rmp::encode as mp;
    let mut buf = Vec::with_capacity(data_msgpack.len() + 32);
    mp::write_map_len(&mut buf, 3).unwrap();
    mp::write_str(&mut buf, "code").unwrap();
    mp::write_uint(&mut buf, 0).unwrap();
    mp::write_str(&mut buf, "msg").unwrap();
    mp::write_str(&mut buf, "success").unwrap();
    mp::write_str(&mut buf, "data").unwrap();
    buf.extend_from_slice(data_msgpack); // data 值 = 列式包(自包含 msgpack value)
    buf
}

/// POST /api/doc/save 请求体。
#[derive(Debug, Deserialize)]
pub struct DocSaveQuery {
    pub domain: String,
    pub application: String,
    pub module: String,
    /// 单据定义文件名；缺失/空时由后端按坐标自动解析默认 DOC 文件（与装载接口一致）。
    #[serde(default)]
    pub file: Option<String>,
}

/// `POST /api/doc/save` —— 回存单据数据（merge/replace 双模式）。
///
/// body: `{ saveMode, changes | snapshot }`（§6.4）。单据坐标走 query 参数。
pub async fn doc_save(
    State(_s): State<CmxAppState>,
    CmxSvrContext(ctx): CmxSvrContext,
    Query(q): Query<DocSaveQuery>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<ApiResp<Value>>> {
    debug!(
        target: "cmx_doc::api",
        handler = "doc_save",
        doc.module = %q.module,
        doc.file = ?q.file,
        "handler invoked"
    );
    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;

    // file 兜底：query 未带 file（前端不传时）→ 按 moduleCode/doc 自动解析默认 DOC 文件，
    // 与装载接口 doc_load_entry 的「file 缺省自动解析」语义一致。
    let file = match &q.file {
        Some(f) if !f.is_empty() && f != "undefined" && f != "null" => f.clone(),
        _ => resolve_doc_file(&q.domain, &q.application, &q.module, None).await?,
    };
    let meta = resolve_doc_meta(
        &q.domain,
        &q.application,
        &q.module,
        Some(file.as_str()),
        None,
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
                return Ok(Json(ApiResp::fail_with_data(
                    422,
                    format!("数据校验未通过（{} 处）", vs.len()),
                    serde_json::json!({ "violations": vs }),
                )));
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
    ctx: &cmx_core::model::service::context::SVRContext,
    doc_file: &str,
    op_override: Option<&str>,
) -> cmx_doc_store_pg::SaveCtx {
    let auth = ctx.auth_context.as_ref();
    let actor_id = auth
        .map(|a| a.user_id.trim())
        .filter(|u| !u.is_empty())
        .and_then(|u| u.parse::<i64>().ok())
        .unwrap_or(0);
    let actor_name = auth
        .map(|a| a.username.trim())
        .filter(|u| !u.is_empty())
        .unwrap_or("系统")
        .to_string();
    cmx_doc_store_pg::SaveCtx {
        actor_id,
        actor_name,
        doc_file: doc_file.to_string(),
        op_override: op_override.map(String::from),
    }
}

/// `POST /api/doc/save/batch` —— 批量回存多单（方案 F）。
///
/// body: `{ atomic?: bool = true, docs: [ { domain, application, module, file, saveMode?, changes|snapshot } ] }`。
/// 一批可混多种单据（每单自带坐标）。`atomic=true` 一个大事务全成全败；`false` 每单独立事务逐单成败。
/// 每单自动享 C（审计）/B1（版本快照）/B2（乐观锁）。
pub async fn doc_save_batch(
    State(_s): State<CmxAppState>,
    CmxSvrContext(ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;

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
        let (domain, app, module) = (get("domain"), get("application"), get("module"));
        let raw_file = get("file");
        // file 兜底：每单 file 缺失/空/脏值时按各自坐标自动解析默认 DOC 文件
        // （与单单路径 /doc/save 的「file 缺省自动解析」契约一致）。
        let file = if !raw_file.is_empty() && raw_file != "undefined" && raw_file != "null" {
            raw_file.to_string()
        } else {
            resolve_doc_file(domain, app, module, None).await?
        };
        let meta = resolve_doc_meta(domain, app, module, Some(file.as_str()), None).await?;
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
fn serde_err_to_api(e: serde_json::Error) -> cmx_api::Error {
    cmx_biz::BizError::internal(format!("序列化保存结果失败: {e}")).into()
}

// ─────────────────── 版本化查询/回滚（方案 §6A.5，Phase 8）───────────────────

/// GET /api/doc/revisions?docFile&rootId —— 列某单全部版本时间线。
#[derive(Debug, Deserialize)]
pub struct RevisionsQuery {
    #[serde(rename = "docFile")]
    pub doc_file: String,
    #[serde(rename = "rootId")]
    pub root_id: String,
    /// 取某版完整快照时用（revision 端点）
    pub rev: Option<i32>,
}

/// `GET /api/doc/revisions` —— 版本时间线。
pub async fn doc_revisions(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Query(q): Query<RevisionsQuery>,
    headers: HeaderMap,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;
    let list = DocRevision::list(mm, &db_id, &q.doc_file, &q.root_id).await?;
    Ok(Json(ApiResp::ok(list)))
}

/// `GET /api/doc/revision` —— 取某历史版完整快照（列式包，前端 fromJSON 直接渲染）。
pub async fn doc_revision(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Query(q): Query<RevisionsQuery>,
    headers: HeaderMap,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;
    let snap = DocRevision::get_snapshot(mm, &db_id, &q.doc_file, &q.root_id, q.rev).await?;
    Ok(Json(ApiResp::ok(snap)))
}

/// `POST /api/doc/restore` —— 把某历史版恢复为新当前版（op=restore，历史不丢，§6A.5）。
///
/// body: `{ docFile, rootId, rev }`。取该版快照 → replace 模式写回。
pub async fn doc_restore(
    State(_s): State<CmxAppState>,
    CmxSvrContext(ctx): CmxSvrContext,
    Query(q): Query<DocSaveQuery>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;

    let root_id = body
        .get("rootId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| cmx_biz::BizError::business("restore 缺少 rootId"))?;
    let rev = body.get("rev").and_then(|v| v.as_i64()).map(|n| n as i32);

    // file 兜底：query 未带 file → 自动解析默认 DOC 文件（与 doc_save 一致）。
    let file = match &q.file {
        Some(f) if !f.is_empty() && f != "undefined" && f != "null" => f.clone(),
        _ => resolve_doc_file(&q.domain, &q.application, &q.module, None).await?,
    };

    // 取历史版快照（列式包）
    let snapshot = DocRevision::get_snapshot(mm, &db_id, &file, root_id, rev).await?;
    if snapshot.is_null() {
        return Err(cmx_biz::BizError::not_found("指定版本不存在").into());
    }

    // 用 replace 模式把快照写回（DocSaver 内部单事务）
    let meta = resolve_doc_meta(
        &q.domain,
        &q.application,
        &q.module,
        Some(file.as_str()),
        None,
    )
    .await?;
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
// cmx-api ⇄ cmx-doc 环）。此处经 use 别名转发，保持本文件内旧调用点不变。
use cmx_model_meta::definitions::resolve::resolve_doc_file;

/// 读单据定义 + base 字段集，解析为 DocMetaView（命中缓存则直接返回）。
///
/// `file` 为 `None` 或空串时，自动调 [`resolve_doc_file`] 选默认/最高版本。
async fn resolve_doc_meta(
    domain: &str,
    app: &str,
    module: &str,
    file: Option<&str>,
    doc: Option<&str>,
) -> Result<Arc<DocMetaView>> {
    // file 兜底：缺失/空/脏值（"undefined"/"null" 等）时自动解析（按 doc 盲选或 moduleCode 精确定位）。
    let file = match file {
        Some(f) if !f.is_empty() && f != "undefined" && f != "null" => f.to_string(),
        _ => resolve_doc_file(domain, app, module, doc).await?,
    };
    let key = cache::doc_key(domain, app, module, &file);
    if let Some(hit) = cache::get(&key) {
        return Ok(hit);
    }

    // 读主定义
    let doc_ref = cmx_model_meta::definitions::store::DefRef {
        domain: Some(domain.to_string()),
        application: Some(app.to_string()),
        app: Some(app.to_string()),
        module: Some(module.to_string()),
        file: Some(file.to_string()),
        id: None,
        kind: None,
    };
    let doc = cmx_model_meta::definitions::store::get_definition(&doc_ref).await?;

    // 读 base 字段集（从 baseDocMetaRef.file 推断；无则空）
    let base = load_base(&doc).await;

    let view = Arc::new(DocMetaView::parse(&doc, &base)?);
    cache::put(key, view.clone());
    Ok(view)
}

/// 从定义的 baseDocMetaRef.file 读 base 字段集（域=base）；失败返回 Null。
async fn load_base(doc: &Value) -> Value {
    let base_file = doc
        .get("baseDocMetaRef")
        .and_then(|r| r.get("file"))
        .and_then(|v| v.as_str());
    let Some(base_file) = base_file else {
        return Value::Null;
    };
    let base_ref = cmx_model_meta::definitions::store::DefRef {
        domain: Some("base".to_string()),
        application: None,
        app: None,
        module: None,
        file: Some(base_file.to_string()),
        id: None,
        kind: None,
    };
    cmx_model_meta::definitions::store::get_definition(&base_ref)
        .await
        .unwrap_or(Value::Null)
}
