//! 业务单据装载/回存 HTTP handler（方案 Phase 4/5）
//!
//! - `GET  /api/doc/data` → DocLoader 装载嵌套 DataSet → ColumnarCodec 列式包 → ApiResp
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

use cmx_biz::doc::{DocLoader, DocMetaView, DocRevision, DocSaver, LoadOptions, cache, saver};
use cmx_core::model::cell::DataValue;
use cmx_core::model::data::dataset::ColumnarCodec;
use cmx_database::get_default_db_manager;

use crate::app_state::CmxAppState;
use crate::middleware::CmxSvrContext;
use crate::rest::header_parse::get_db_id_from_header;
use crate::{ApiResp, Result};

/// GET /api/doc/data 查询参数。
#[derive(Debug, Deserialize)]
pub struct DocDataQuery {
    pub domain: String,
    pub application: String,
    pub module: String,
    /// 单据定义文件名（如 cmxfico_doc_meta_v1.json）
    pub file: String,
    /// 可选：根层过滤 `col:value`（简单等值）
    #[serde(default)]
    pub filter: Option<String>,
    /// 可选：根层限制行数
    #[serde(default)]
    pub limit: Option<u64>,
    /// 可选：装载深度（懒下钻）
    #[serde(default)]
    pub depth: Option<usize>,
}

/// `GET /api/doc/data` —— 装载单据数据为列式包。
pub async fn doc_data(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Query(q): Query<DocDataQuery>,
    headers: HeaderMap,
) -> Result<Json<ApiResp<Value>>> {
    debug!("{:<12} - doc_data {}/{}", "HANDLER", q.module, q.file);
    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;

    // 1. 取（或建）DocMetaView，带进程内缓存
    let meta = resolve_doc_meta(&q.domain, &q.application, &q.module, &q.file).await?;

    // 2. 组装装载选项
    let mut opts = LoadOptions {
        root_limit: q.limit,
        depth: q.depth,
        ..Default::default()
    };
    if let Some(f) = &q.filter {
        if let Some((col, val)) = f.split_once(':') {
            opts.root_filter
                .push((col.to_string(), DataValue::String(val.to_string())));
        }
    }

    // 3. 装载 → 列式包
    let ds = DocLoader::load(mm, &db_id, &meta, &opts).await?;
    let pkg = ColumnarCodec::encode(&ds);

    Ok(Json(ApiResp::ok(pkg)))
}

/// `GET /api/doc/data.bin` —— 装载单据数据为**列式二进制包**(msgpack,零拷贝新链路)。
///
/// 与 [`doc_data`] 同参数、同 columnar 结构,但:
/// - 走 tokio-postgres + `ZmcDocLoader`(持有原始 Row,零拷贝);
/// - 出口是 msgpack 二进制信封 `{code, msg, data:<列式包>}`,`Content-Type: application/x-msgpack`;
/// - 前端 `msgpack.decode` 后结构与 JSON 版逐字段同构,`CmxDataSet.fromJSON` 无改动复用。
pub async fn doc_data_bin(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Query(q): Query<DocDataQuery>,
    headers: HeaderMap,
) -> Result<axum::response::Response> {
    use axum::response::IntoResponse;
    use cmx_biz::doc::ZmcDocLoader;
    use cmx_database_pg::get_default_pg_db_manager;

    debug!("{:<12} - doc_data_bin {}/{}", "HANDLER", q.module, q.file);
    let mm = get_default_pg_db_manager();
    let db_id = get_db_id_from_header(&headers).await;

    let meta = resolve_doc_meta(&q.domain, &q.application, &q.module, &q.file).await?;

    let mut opts = LoadOptions {
        root_limit: q.limit,
        depth: q.depth,
        ..Default::default()
    };
    if let Some(f) = &q.filter {
        if let Some((col, val)) = f.split_once(':') {
            opts.root_filter
                .push((col.to_string(), DataValue::String(val.to_string())));
        }
    }

    // 装载 → 列式二进制包
    let zmc = ZmcDocLoader::load(mm, &db_id, &meta, &opts).await?;
    let mut columnar = Vec::new();
    zmc.encode_columnar_binary(&mut columnar);

    // 包成信封 msgpack:{code:0, msg:"success", data:<列式包>}
    // 列式包本身已是合法 msgpack value,写完 "data" 键后直接追加其字节。
    let body = encode_envelope_ok(&columnar);

    Ok((
        [(axum::http::header::CONTENT_TYPE, "application/x-msgpack")],
        body,
    )
        .into_response())
}

/// `GET /api/doc/data.sqlx.bin` —— 装载单据数据为**列式二进制包**(msgpack),但走 **sqlx** 驱动。
///
/// 与 [`doc_data_bin`] 逐字节同构的出口(同 columnar 结构、同 msgpack 信封、同
/// `Content-Type: application/x-msgpack`),唯一差异是数据通道:
/// - 走 sqlx(`cmx-database`) + [`ZmcDocLoaderSqlx`],持有原始 sqlx PgRow 零拷贝编码;
/// - 而 [`doc_data_bin`] 走 tokio-postgres(`cmx-database-pg`) + `ZmcDocLoader`。
///
/// 供「Sqlx + ZmcDataSet + 二进制」的第三套实现使用。前端 `msgpack.decode` 后与 JSON 版同构,
/// `CmxDataSet.fromJSON` 无改动复用。
pub async fn doc_data_bin_sqlx(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Query(q): Query<DocDataQuery>,
    headers: HeaderMap,
) -> Result<axum::response::Response> {
    use axum::response::IntoResponse;
    use cmx_biz::doc::ZmcDocLoaderSqlx;

    debug!(
        "{:<12} - doc_data_bin_sqlx {}/{}",
        "HANDLER", q.module, q.file
    );
    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;

    let meta = resolve_doc_meta(&q.domain, &q.application, &q.module, &q.file).await?;

    let mut opts = LoadOptions {
        root_limit: q.limit,
        depth: q.depth,
        ..Default::default()
    };
    if let Some(f) = &q.filter {
        if let Some((col, val)) = f.split_once(':') {
            opts.root_filter
                .push((col.to_string(), DataValue::String(val.to_string())));
        }
    }

    // 装载(sqlx 零拷贝)→ 列式二进制包
    let zmc = ZmcDocLoaderSqlx::load(mm, &db_id, &meta, &opts).await?;
    let mut columnar = Vec::new();
    zmc.encode_columnar_binary(&mut columnar);

    let body = encode_envelope_ok(&columnar);

    Ok((
        [(axum::http::header::CONTENT_TYPE, "application/x-msgpack")],
        body,
    )
        .into_response())
}

/// `GET /api/doc/data.zmc.json` —— 用 **ZmcDataSet(零拷贝)** 装载,但出口是**纯 JSON 列式包**
/// (走普通 ApiResp 信封,`Content-Type: application/json`),前端**无需 msgpack 解码**,
/// 直接 `CmxDataSet.fromJSON`。第五套实现。
///
/// 与各路对比:
/// - [`doc_data`]        : sqlx + 老 `DataSet` → `ColumnarCodec` JSON(老链路)
/// - [`doc_data_bin`]    : tokio + `ZmcDataSet` → msgpack 二进制
/// - **本 handler**      : tokio + `ZmcDataSet` → **JSON**(零拷贝装载,JSON 出口)
///
/// 即与 [`doc_data_bin`] 同一 tokio 零拷贝装载器,仅把出口 `encode_columnar_binary`
/// 换成 `encode_columnar_json`。产出与老 `doc_data` 逐字段同构,前端组件零改动复用。
pub async fn doc_data_zmc_json(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Query(q): Query<DocDataQuery>,
    headers: HeaderMap,
) -> Result<Json<ApiResp<Value>>> {
    use cmx_biz::doc::ZmcDocLoader;
    use cmx_database_pg::get_default_pg_db_manager;

    debug!(
        "{:<12} - doc_data_zmc_json {}/{}",
        "HANDLER", q.module, q.file
    );
    let mm = get_default_pg_db_manager();
    let db_id = get_db_id_from_header(&headers).await;

    let meta = resolve_doc_meta(&q.domain, &q.application, &q.module, &q.file).await?;

    let mut opts = LoadOptions {
        root_limit: q.limit,
        depth: q.depth,
        ..Default::default()
    };
    if let Some(f) = &q.filter {
        if let Some((col, val)) = f.split_once(':') {
            opts.root_filter
                .push((col.to_string(), DataValue::String(val.to_string())));
        }
    }

    // 装载(tokio 零拷贝)→ 纯 JSON 列式包
    let zmc = ZmcDocLoader::load(mm, &db_id, &meta, &opts).await?;
    let pkg = zmc.encode_columnar_json();

    Ok(Json(ApiResp::ok(pkg)))
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
    debug!("{:<12} - doc_meta {}/{}", "HANDLER", q.module, q.file);

    let meta = resolve_doc_meta(&q.domain, &q.application, &q.module, &q.file).await?;
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
fn column_to_json(c: &cmx_biz::doc::ColumnView) -> Value {
    serde_json::json!({
        "name": c.name,
        "caption": c.caption,
        "dataType": c.data_type,
        "dimType": c.dim_type,
        "agg": c.agg,
        "nullable": c.nullable,
        "isPrimaryKey": c.is_primary_key,
    })
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
    pub file: String,
}

/// `POST /api/doc/save` —— 回存单据数据（merge/replace 双模式）。
///
/// body: `{ saveMode, changes | snapshot }`（§6.4）。单据坐标走 query 参数。
pub async fn doc_save(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Query(q): Query<DocSaveQuery>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<ApiResp<Value>>> {
    debug!("{:<12} - doc_save {}/{}", "HANDLER", q.module, q.file);
    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;

    let meta = resolve_doc_meta(&q.domain, &q.application, &q.module, &q.file).await?;
    let (mode, changes) = saver::parse_save_body(&body);

    // §14.2 后端二次校验：对 changeset 各行跑 validationRules，error 阻断保存。
    if !meta.validation_rules.is_empty() {
        if let Some(vr) = run_validation(&meta, &changes) {
            return Ok(Json(ApiResp::ok(vr)));
        }
    }

    let result = DocSaver::save(mm, &db_id, &meta, mode, &changes).await?;

    Ok(Json(ApiResp::ok(
        serde_json::to_value(result).map_err(serde_err_to_api)?,
    )))
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
                    let res = cmx_biz::doc::validate(&meta.validation_rules, &scope);
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
fn build_row_scope(row: &Value) -> cmx_biz::doc::Scope {
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
    cmx_biz::doc::scope_from_json(&Value::Object(flat))
}

/// serde_json 序列化错误 → api Error（局部小工具）。
fn serde_err_to_api(e: serde_json::Error) -> crate::Error {
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
    CmxSvrContext(_ctx): CmxSvrContext,
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

    // 取历史版快照（列式包）
    let snapshot = DocRevision::get_snapshot(mm, &db_id, &q.file, root_id, rev).await?;
    if snapshot.is_null() {
        return Err(cmx_biz::BizError::not_found("指定版本不存在").into());
    }

    // 用 replace 模式把快照写回（DocSaver 内部单事务）
    let meta = resolve_doc_meta(&q.domain, &q.application, &q.module, &q.file).await?;
    // 快照是列式包 { datasetId, columns, rows, childRows }；replace 期望 { table:{rows:[{id,upper_id,fields}]} }
    // 这里把列式包转成 replace 输入（简化：交给 DocSaver 前先归一）
    let replace_input = columnar_to_replace_input(&snapshot);
    let result = DocSaver::save(
        mm,
        &db_id,
        &meta,
        cmx_biz::doc::SaveMode::Replace,
        &replace_input,
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

/// 读单据定义 + base 字段集，解析为 DocMetaView（命中缓存则直接返回）。
async fn resolve_doc_meta(
    domain: &str,
    app: &str,
    module: &str,
    file: &str,
) -> Result<Arc<DocMetaView>> {
    let key = cache::doc_key(domain, app, module, file);
    if let Some(hit) = cache::get(&key) {
        return Ok(hit);
    }

    // 读主定义
    let doc_ref = cmx_portal::definitions::store::DefRef {
        domain: Some(domain.to_string()),
        application: Some(app.to_string()),
        app: Some(app.to_string()),
        module: Some(module.to_string()),
        file: Some(file.to_string()),
        id: None,
    };
    let doc = cmx_portal::definitions::store::get_definition(&doc_ref).await?;

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
    let base_ref = cmx_portal::definitions::store::DefRef {
        domain: Some("base".to_string()),
        application: None,
        app: None,
        module: None,
        file: Some(base_file.to_string()),
        id: None,
    };
    cmx_portal::definitions::store::get_definition(&base_ref)
        .await
        .unwrap_or(Value::Null)
}
