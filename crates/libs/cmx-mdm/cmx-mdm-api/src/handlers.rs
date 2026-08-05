//! MDM handlers —— M0 健康检查 + M1 激活映射配置 CRUD + 手动激活。
//!
//! 提取器惯例（对齐 cmx-dct-api/src/handlers.rs:14-27）：
//!   - `State(_s): State<CmxAppState>`：状态（DB 走全局单例，常忽略为 `_s`）
//!   - `CmxSvrContext(_ctx)`：cmx 上下文
//!   - `headers: HeaderMap`：取 db_id（与 dct/doc 同库路由一致）
//!   - `Query<T>` / `Json<T>`：参数（**禁用 `Path`**，承接 AGENTS.md §四 第 5 条）

use axum::Json;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use serde_json::{json, Value};

use cmx_api::CmxAppState;
use cmx_api::middleware::CmxSvrContext;
use cmx_api::{ApiResp, Result};

use cmx_database_pg::get_default_pg_db_manager;
use cmx_dct_store_pg::resolve_db_id;
use cmx_mdm_model::activation::ActivationConfig;
use cmx_mdm_model::codegen::RandomCodeGenerator;
use cmx_mdm_store_pg as store;

/// MDM 模块健康检查。
pub async fn mdm_health(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
) -> Result<Json<ApiResp<Value>>> {
    Ok(Json(ApiResp::ok(json!({ "module": "mdm", "status": "ok" }))))
}

/// 从 headers 取 db_id（对齐 cmx-dct-api 的 resolve_db_id 用法）。
fn db_id_from_headers(headers: &HeaderMap) -> Option<String> {
    headers
        .get("db_id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
}

/// 激活映射列表（配置器 UI 用）。
pub async fn mdm_activations_list(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    headers: HeaderMap,
    Query(q): Query<ActivationListQuery>,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_pg_db_manager();
    let db_id = resolve_db_id(db_id_from_headers(&headers).as_deref()).await;
    let list = store::list(
        mm,
        &db_id,
        q.source_doc_type.as_deref(),
        q.cr_type.as_deref(),
    )
    .await?;
    Ok(Json(ApiResp::ok(json!(list))))
}

/// 保存激活映射（upsert by activation_code）。配置器 UI 用。
pub async fn mdm_activations_save(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(body): Json<ActivationConfig>,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_pg_db_manager();
    let db_id = resolve_db_id(db_id_from_headers(&headers).as_deref()).await;
    let code = store::upsert(mm, &db_id, &body).await?;
    Ok(Json(ApiResp::ok(json!({ "activationCode": code }))))
}

/// 手动触发激活（审批型 CR 兜底入口 / 内部 CR 直接调）。
pub async fn mdm_cr_activate(
    State(_s): State<CmxAppState>,
    CmxSvrContext(svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(body): Json<ActivateBody>,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_pg_db_manager();
    let db_id = resolve_db_id(db_id_from_headers(&headers).as_deref()).await;
    let operated_by = svr_ctx
        .auth_context
        .as_ref()
        .and_then(|a| a.user_id.parse::<i64>().ok())
        .unwrap_or(0);
    let codegen = RandomCodeGenerator;
    let record_id = store::activate(mm, &db_id, body.cr_id, operated_by, &codegen).await?;
    Ok(Json(ApiResp::ok(json!({ "recordId": record_id }))))
}

#[derive(serde::Deserialize)]
pub struct ActivationListQuery {
    #[serde(default, alias = "sourceDocType")]
    pub source_doc_type: Option<String>,
    #[serde(default, alias = "crType")]
    pub cr_type: Option<String>,
}

#[derive(serde::Deserialize)]
pub struct ActivateBody {
    #[serde(alias = "crId")]
    pub cr_id: i64,
}

// ════════════════════════════════════════════════════════════════════════════
// M2 · CR 变更请求:新建 / 审批流转 / 列表 / 详情
// ════════════════════════════════════════════════════════════════════════════

/// 新建 draft CR(录入台用)。body: { head: {...}, lines: [...] }
pub async fn mdm_cr_create(
    State(_s): State<CmxAppState>,
    CmxSvrContext(svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(body): Json<CreateBody>,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_pg_db_manager();
    let db_id = resolve_db_id(db_id_from_headers(&headers).as_deref()).await;
    let operated_by = svr_ctx
        .auth_context
        .as_ref()
        .and_then(|a| a.user_id.parse::<i64>().ok())
        .unwrap_or(0);
    let cr_id = store::create_cr(mm, &db_id, &body.head, &body.lines, operated_by).await?;
    Ok(Json(ApiResp::ok(json!({ "crId": cr_id, "status": "draft" }))))
}

/// 提交审批:draft → approving
pub async fn mdm_cr_submit(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(body): Json<CrIdBody>,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_pg_db_manager();
    let db_id = resolve_db_id(db_id_from_headers(&headers).as_deref()).await;
    store::check_status(mm, &db_id, None, body.cr_id, "draft").await?;
    store::set_cr_status_pub(mm, &db_id, None, body.cr_id, "approving").await?;
    Ok(Json(ApiResp::ok(json!({ "crId": body.cr_id, "status": "approving" }))))
}

/// 审批通过:approving → 激活器单事务 → activated。
/// 方案 A:直接对 approving 的 CR 调激活器(激活器接受 approving),失败回滚到 approving。
pub async fn mdm_cr_approve(
    State(_s): State<CmxAppState>,
    CmxSvrContext(svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(body): Json<CrIdBody>,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_pg_db_manager();
    let db_id = resolve_db_id(db_id_from_headers(&headers).as_deref()).await;
    store::check_status(mm, &db_id, None, body.cr_id, "approving").await?;
    let operated_by = svr_ctx
        .auth_context
        .as_ref()
        .and_then(|a| a.user_id.parse::<i64>().ok())
        .unwrap_or(0);
    let codegen = RandomCodeGenerator;
    let record_id = store::activate(mm, &db_id, body.cr_id, operated_by, &codegen).await?;
    Ok(Json(ApiResp::ok(
        json!({ "crId": body.cr_id, "status": "activated", "recordId": record_id }),
    )))
}

/// 驳回:approving → rejected(cm_* 全程不动)
pub async fn mdm_cr_reject(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(body): Json<RejectBody>,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_pg_db_manager();
    let db_id = resolve_db_id(db_id_from_headers(&headers).as_deref()).await;
    store::check_status(mm, &db_id, None, body.cr_id, "approving").await?;
    store::set_cr_status_pub(mm, &db_id, None, body.cr_id, "rejected").await?;
    Ok(Json(ApiResp::ok(json!({ "crId": body.cr_id, "status": "rejected" }))))
}

/// 驳回复活:rejected → 克隆新 draft(source_cr_id 指向旧)
pub async fn mdm_cr_clone_revise(
    State(_s): State<CmxAppState>,
    CmxSvrContext(svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(body): Json<CrIdBody>,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_pg_db_manager();
    let db_id = resolve_db_id(db_id_from_headers(&headers).as_deref()).await;
    let operated_by = svr_ctx
        .auth_context
        .as_ref()
        .and_then(|a| a.user_id.parse::<i64>().ok())
        .unwrap_or(0);
    let new_id = store::clone_revise(mm, &db_id, body.cr_id, operated_by).await?;
    Ok(Json(ApiResp::ok(
        json!({ "newCrId": new_id, "sourceCrId": body.cr_id, "status": "draft" }),
    )))
}

/// 作废:draft → aborted
pub async fn mdm_cr_abort(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(body): Json<CrIdBody>,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_pg_db_manager();
    let db_id = resolve_db_id(db_id_from_headers(&headers).as_deref()).await;
    store::abort_cr(mm, &db_id, body.cr_id).await?;
    Ok(Json(ApiResp::ok(json!({ "crId": body.cr_id, "status": "aborted" }))))
}

/// CR 列表(query: ?docStatus=)
pub async fn mdm_cr_list(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    headers: HeaderMap,
    Query(q): Query<CrListQuery>,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_pg_db_manager();
    let db_id = resolve_db_id(db_id_from_headers(&headers).as_deref()).await;
    let list = store::list_cr(mm, &db_id, q.doc_status.as_deref()).await?;
    Ok(Json(ApiResp::ok(json!(list))))
}

/// CR 详情(query: ?crId=,返回头+行)
pub async fn mdm_cr_detail(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    headers: HeaderMap,
    Query(q): Query<CrDetailQuery>,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_pg_db_manager();
    let db_id = resolve_db_id(db_id_from_headers(&headers).as_deref()).await;
    let detail = store::get_cr_detail(mm, &db_id, q.cr_id).await?;
    Ok(Json(ApiResp::ok(detail)))
}

// ── M2 body struct ───────────────────────────────────────────────────────────

/// 通用 CR id body(submit/approve/reject/clone-revise/abort 复用)。
#[derive(serde::Deserialize)]
pub struct CrIdBody {
    #[serde(alias = "crId")]
    pub cr_id: i64,
}

/// 驳回(带可选原因)。
#[derive(serde::Deserialize)]
pub struct RejectBody {
    #[serde(alias = "crId")]
    pub cr_id: i64,
    #[serde(default)]
    pub reason: Option<String>,
}

/// 新建 CR(头+行)。
#[derive(serde::Deserialize)]
pub struct CreateBody {
    pub head: Value,
    #[serde(default)]
    pub lines: Vec<Value>,
}

/// CR 列表查询。
#[derive(serde::Deserialize)]
pub struct CrListQuery {
    #[serde(default, alias = "docStatus")]
    pub doc_status: Option<String>,
}

/// CR 详情查询。
#[derive(serde::Deserialize)]
pub struct CrDetailQuery {
    #[serde(alias = "crId")]
    pub cr_id: i64,
}

// ════════════════════════════════════════════════════════════════════════════
// M3 · 匹配合并：find-duplicates / merge-requests / undo
// ════════════════════════════════════════════════════════════════════════════

use cmx_mdm_model::match_algo::{find_candidates, FieldKind, MatchFieldSpec};
use cmx_mdm_model::survivorship::SurvivorRule;
use std::collections::HashMap;

/// dict → 物理表/明细表解析（M3 MVP 注册表；M6 多域改走 DCT meta tableName）。
fn dict_tables(dict_code: &str) -> Option<(String, Vec<(String, String)>)> {
    match dict_code {
        "supplier" => Some((
            "cm_supplier".into(),
            vec![("cm_bank_account".into(), "supplier_id".into())],
        )),
        _ => None,
    }
}

/// 默认比较/存活字段（supplier）。
fn default_specs() -> Vec<MatchFieldSpec> {
    vec![
        MatchFieldSpec { field: "credit_code".into(), weight: 40, kind: FieldKind::Exact },
        MatchFieldSpec { field: "tax_no".into(), weight: 30, kind: FieldKind::Exact },
        MatchFieldSpec { field: "name".into(), weight: 30, kind: FieldKind::EditDistance },
    ]
}
fn default_cluster_keys() -> Vec<&'static str> {
    vec!["credit_code", "tax_no", "name"]
}
fn default_survive_fields() -> Vec<String> {
    ["name", "tax_no", "credit_code", "short_name", "phone"]
        .iter()
        .map(|s| s.to_string())
        .collect()
}
fn load_columns() -> Vec<&'static str> {
    vec!["id", "name", "tax_no", "credit_code", "short_name", "phone", "update_time"]
}

/// 实时查重。body { dictCode, recordId }。
pub async fn mdm_find_duplicates(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(body): Json<FindDupBody>,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_pg_db_manager();
    let db_id = resolve_db_id(db_id_from_headers(&headers).as_deref()).await;
    let (head_table, _lines) = dict_tables(&body.dict_code)
        .ok_or_else(|| store::api_err(&format!("字典 {} 未配置表映射", body.dict_code)))?;

    let all = store::load_published(mm, &db_id, &head_table, &load_columns()).await?;
    let target = all
        .iter()
        .find(|r| r.id == body.record_id)
        .cloned()
        .ok_or_else(|| store::api_err(&format!("记录 {} 不存在或非 published", body.record_id)))?;

    let specs = default_specs();
    let keys = default_cluster_keys();
    let candidates = find_candidates(&target, &all, &specs, &keys);

    // 承载 match_group（pending），供后续合并/评审
    let member_ids: Vec<i64> = std::iter::once(target.id)
        .chain(candidates.iter().map(|c| c.record_id))
        .collect();
    let top_score = candidates.first().map(|c| c.score as i64).unwrap_or(0);
    // 审查 C2：查重目标默认 master，管家 UI 可改选
    let group_id = store::insert_match_group(
        mm, &db_id, None, &body.dict_code, &format!("dup:{}", target.id),
        &json!(member_ids), Some(target.id), top_score,
        candidates.first().map(|c| format!("{:?}", c.decision).to_lowercase()).as_deref().unwrap_or("nomatch"),
        "pending",
    )
    .await?;

    Ok(Json(ApiResp::ok(json!({
        "matchGroupId": group_id,
        "targetId": target.id,
        "candidates": candidates.iter().map(|c| json!({
            "recordId": c.record_id,
            "score": c.score,
            "decision": format!("{:?}", c.decision),
        })).collect::<Vec<_>>(),
    }))))
}

/// 合并请求列表。GET ?dictCode=&status=。
pub async fn mdm_merge_requests_list(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    headers: HeaderMap,
    Query(q): Query<MergeListQuery>,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_pg_db_manager();
    let db_id = resolve_db_id(db_id_from_headers(&headers).as_deref()).await;
    let list = store::list_match_groups(mm, &db_id, q.dict_code.as_deref(), q.status.as_deref())
        .await?;
    Ok(Json(ApiResp::ok(json!(list))))
}

/// 确认合并。body { dictCode, masterId, victimIds, survivorship? }。
pub async fn mdm_merge_requests_create(
    State(_s): State<CmxAppState>,
    CmxSvrContext(svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(body): Json<MergeBody>,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_pg_db_manager();
    let db_id = resolve_db_id(db_id_from_headers(&headers).as_deref()).await;
    let (head_table, line_tables) = dict_tables(&body.dict_code)
        .ok_or_else(|| store::api_err(&format!("字典 {} 未配置表映射", body.dict_code)))?;
    let operated_by = svr_ctx
        .auth_context
        .as_ref()
        .and_then(|a| a.user_id.parse::<i64>().ok())
        .unwrap_or(0);

    // 审查 C1：管家路径带 mergeId 复用 group（不新插）；否则新插 pending
    let member_ids: Vec<i64> = std::iter::once(body.master_id)
        .chain(body.victim_ids.clone())
        .collect();
    let group_id = match body.merge_id {
        Some(g) => g,
        None => {
            store::insert_match_group(
                mm, &db_id, None, &body.dict_code, &format!("merge:{}", body.master_id),
                &json!(member_ids), Some(body.master_id), 100, "automerge", "pending",
            )
            .await?
        }
    };

    // 审查 A1：未知 survivorship 规则报错（禁止静默兜底）；选 victim/手填走 overrides
    let mut rules: HashMap<String, SurvivorRule> = HashMap::new();
    if let Some(m) = body.survivorship.as_ref() {
        for (k, v) in m {
            let r = match v.as_str() {
                Some("master") => SurvivorRule::MasterFirst,
                Some("fullest") => SurvivorRule::Fullest,
                Some("latest") => SurvivorRule::Latest,
                other => {
                    return Err(store::api_err(&format!(
                        "字段 {k} 的 survivorship 规则 {other:?} 不合法（master/fullest/latest；选 victim/手填请走 overrides）"
                    )))
                }
            };
            rules.insert(k.clone(), r);
        }
    }
    let overrides = body.overrides.clone().unwrap_or_default();

    let master_id = store::merge(
        mm, &db_id, &body.dict_code, &head_table, body.master_id, &body.victim_ids,
        &default_survive_fields(), &rules, &overrides, &line_tables, operated_by, group_id,
    )
    .await?;

    Ok(Json(ApiResp::ok(json!({ "masterId": master_id, "matchGroupId": group_id }))))
}

/// 合并请求详情（红线 diff 用）。GET ?mergeId=。返回 group+master+victims。
pub async fn mdm_merge_request_detail(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    headers: HeaderMap,
    Query(q): Query<UndoBody>,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_pg_db_manager();
    let db_id = resolve_db_id(db_id_from_headers(&headers).as_deref()).await;
    let mut group = store::get_match_group(mm, &db_id, q.merge_id)
        .await?
        .ok_or_else(|| store::api_err(&format!("合并请求 {} 不存在", q.merge_id)))?;
    // 审查 B2：group 的 JSONB 列 parse 成对象再吐
    for f in ["member_ids", "survivorship_log"] {
        if let Some(Value::String(s)) = group.get(f).cloned() {
            if let Ok(p) = serde_json::from_str::<Value>(&s) {
                group[f] = p;
            }
        }
    }
    let dict_code = group.get("dict_code").and_then(|v| v.as_str()).unwrap_or("supplier").to_string();
    let master_id = group.get("master_id").and_then(|v| v.as_i64()).unwrap_or(0);
    let (head_table, _lines) = dict_tables(&dict_code)
        .ok_or_else(|| store::api_err(&format!("字典 {dict_code} 未配置表映射")))?;
    let victim_ids: Vec<i64> = group
        .get("member_ids")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|m| m.as_i64().filter(|id| *id != master_id)).collect())
        .unwrap_or_default();
    let cols = load_columns();
    let master = store::load_by_ids(mm, &db_id, None, &head_table, &cols, &[master_id])
        .await?
        .pop()
        .map(|r| r.fields)
        .unwrap_or_default();
    let victims = store::load_by_ids(mm, &db_id, None, &head_table, &cols, &victim_ids)
        .await?
        .into_iter()
        .map(|r| r.fields)
        .collect::<Vec<_>>();
    Ok(Json(ApiResp::ok(json!({ "group": group, "master": master, "victims": victims }))))
}

/// 驳回合并请求。body { mergeId, reason? }。CAS pending→rejected + 审计（审查 C3/C5）。
pub async fn mdm_merge_request_reject(
    State(_s): State<CmxAppState>,
    CmxSvrContext(svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(body): Json<RejectMergeBody>,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_pg_db_manager();
    let db_id = resolve_db_id(db_id_from_headers(&headers).as_deref()).await;
    let operated_by = svr_ctx
        .auth_context
        .as_ref()
        .and_then(|a| a.user_id.parse::<i64>().ok())
        .unwrap_or(0);
    let txn_ctx = mm.get_transaction_context();
    let guard = txn_ctx.begin_with_guard(&db_id).await
        .map_err(|e| store::api_err(&format!("开事务失败: {e}")))?;
    let txn_id = guard.txn_id().to_string();
    let n = store::transition_match_group(mm, &db_id, Some(&txn_id), body.merge_id, "pending", "rejected")
        .await?;
    if n == 0 {
        return Err(store::api_err(&format!("group {} 非 pending，不可驳回", body.merge_id)));
    }
    // 留痕：驳回人 + 原因存 group.survivorship_log（pending 时为 NULL，不覆盖既有 slog）
    let log = json!({ "rejected_by": operated_by, "reason": body.reason });
    store::update_match_group(mm, &db_id, Some(&txn_id), body.merge_id, "rejected", Some(&log), None)
        .await?;
    guard.commit().await.map_err(|e| store::api_err(&format!("提交失败: {e}")))?;
    Ok(Json(ApiResp::ok(json!({ "mergeId": body.merge_id, "status": "rejected" }))))
}

/// unmerge。body { mergeId }。
pub async fn mdm_merge_requests_undo(
    State(_s): State<CmxAppState>,
    CmxSvrContext(svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(body): Json<UndoBody>,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_pg_db_manager();
    let db_id = resolve_db_id(db_id_from_headers(&headers).as_deref()).await;
    let group = store::get_match_group(mm, &db_id, body.merge_id)
        .await?
        .ok_or_else(|| store::api_err(&format!("合并请求 {} 不存在", body.merge_id)))?;
    let dict_code = group.get("dict_code").and_then(|v| v.as_str()).unwrap_or("supplier").to_string();
    let master_id = group.get("master_id").and_then(|v| v.as_i64()).unwrap_or(0);
    let (head_table, line_tables) = dict_tables(&dict_code)
        .ok_or_else(|| store::api_err(&format!("字典 {dict_code} 未配置表映射")))?;
    // victim = member_ids 中非 master 的第一个（JSONB 列 to_json_value 为转义字符串，需 parse）
    let members_raw = group.get("member_ids").cloned().unwrap_or(Value::Null);
    let members = match members_raw {
        Value::String(s) => serde_json::from_str::<Value>(&s).unwrap_or(Value::Null),
        v => v,
    };
    let victim_id = members
        .as_array()
        .and_then(|arr| arr.iter().find_map(|m| m.as_i64().filter(|id| *id != master_id)))
        .unwrap_or(0);
    let operated_by = svr_ctx
        .auth_context
        .as_ref()
        .and_then(|a| a.user_id.parse::<i64>().ok())
        .unwrap_or(0);

    store::unmerge(
        mm, &db_id, &dict_code, &head_table, master_id, victim_id, &line_tables,
        operated_by, body.merge_id,
    )
    .await?;
    Ok(Json(ApiResp::ok(json!({ "masterId": master_id, "victimId": victim_id, "status": "unmerged" }))))
}

// ── M3 body struct ───────────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct FindDupBody {
    #[serde(alias = "dictCode")]
    pub dict_code: String,
    #[serde(alias = "recordId")]
    pub record_id: i64,
}

#[derive(serde::Deserialize)]
pub struct MergeListQuery {
    #[serde(default, alias = "dictCode")]
    pub dict_code: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(serde::Deserialize)]
pub struct MergeBody {
    #[serde(alias = "dictCode")]
    pub dict_code: String,
    #[serde(alias = "masterId")]
    pub master_id: i64,
    #[serde(default, alias = "victimIds")]
    pub victim_ids: Vec<i64>,
    /// 管家路径复用 group（审查 C1）；不传则新插
    #[serde(default, alias = "mergeId")]
    pub merge_id: Option<i64>,
    #[serde(default)]
    pub survivorship: Option<serde_json::Map<String, Value>>,
    /// 人工裁决显式真值（选 victim/手填，审查 A1/A2）；键 ⊆ survive_fields
    #[serde(default)]
    pub overrides: Option<serde_json::Map<String, Value>>,
}

#[derive(serde::Deserialize)]
pub struct UndoBody {
    #[serde(alias = "mergeId")]
    pub merge_id: i64,
}

#[derive(serde::Deserialize)]
pub struct RejectMergeBody {
    #[serde(alias = "mergeId")]
    pub merge_id: i64,
    #[serde(default)]
    pub reason: Option<String>,
}
