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
use cmx_api::actor::actor_id_i64;
use cmx_api::db_id::resolve_db_id_from_headers;
use cmx_api::middleware::CmxSvrContext;
use cmx_api::{ApiResp, Result};

use cmx_database_pg::{get_default_pg_db_manager, DatabaseManager};
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

/// 激活映射列表（配置器 UI 用）。
pub async fn mdm_activations_list(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    headers: HeaderMap,
    Query(q): Query<ActivationListQuery>,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_pg_db_manager();
    let db_id = resolve_db_id_from_headers(&headers).await;
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
    let db_id = resolve_db_id_from_headers(&headers).await;
    let code = store::upsert(mm, &db_id, &body).await?;
    Ok(Json(ApiResp::ok(json!({ "activationCode": code }))))
}

/// 手动触发激活（审批型 CR 兜底入口 / 内部 CR 直接调）。
pub async fn mdm_cr_activate(
    State(_s): State<CmxAppState>,
    svr_ctx: CmxSvrContext,
    headers: HeaderMap,
    Json(body): Json<ActivateBody>,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_pg_db_manager();
    let db_id = resolve_db_id_from_headers(&headers).await;
    let operated_by = actor_id_i64(&svr_ctx);
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

/// 提交审批:draft → approving
pub async fn mdm_cr_submit(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(body): Json<CrIdBody>,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_pg_db_manager();
    let db_id = resolve_db_id_from_headers(&headers).await;
    store::check_status(mm, &db_id, None, body.cr_id, "draft").await?;
    store::set_cr_status_pub(mm, &db_id, None, body.cr_id, "approving").await?;
    Ok(Json(ApiResp::ok(json!({ "crId": body.cr_id, "status": "approving" }))))
}

/// 审批通过:approving → 激活器单事务 → activated。
/// 方案 A:直接对 approving 的 CR 调激活器(激活器接受 approving),失败回滚到 approving。
pub async fn mdm_cr_approve(
    State(_s): State<CmxAppState>,
    svr_ctx: CmxSvrContext,
    headers: HeaderMap,
    Json(body): Json<CrIdBody>,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_pg_db_manager();
    let db_id = resolve_db_id_from_headers(&headers).await;
    store::check_status(mm, &db_id, None, body.cr_id, "approving").await?;
    let operated_by = actor_id_i64(&svr_ctx);
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
    let db_id = resolve_db_id_from_headers(&headers).await;
    store::check_status(mm, &db_id, None, body.cr_id, "approving").await?;
    store::set_cr_status_pub(mm, &db_id, None, body.cr_id, "rejected").await?;
    Ok(Json(ApiResp::ok(json!({ "crId": body.cr_id, "status": "rejected" }))))
}

/// 驳回复活:rejected → 克隆新 draft(source_cr_id 指向旧)
pub async fn mdm_cr_clone_revise(
    State(_s): State<CmxAppState>,
    svr_ctx: CmxSvrContext,
    headers: HeaderMap,
    Json(body): Json<CrIdBody>,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_pg_db_manager();
    let db_id = resolve_db_id_from_headers(&headers).await;
    let operated_by = actor_id_i64(&svr_ctx);
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
    let db_id = resolve_db_id_from_headers(&headers).await;
    store::abort_cr(mm, &db_id, body.cr_id).await?;
    Ok(Json(ApiResp::ok(json!({ "crId": body.cr_id, "status": "aborted" }))))
}

/// CR 列表(query: ?docStatus=&withPayload=)
pub async fn mdm_cr_list(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    headers: HeaderMap,
    Query(q): Query<CrListQuery>,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_pg_db_manager();
    let db_id = resolve_db_id_from_headers(&headers).await;
    let (list, total) =
        store::list_cr(mm, &db_id, q.doc_status.as_deref(), q.page, q.page_size, q.with_payload)
            .await?;
    Ok(Json(ApiResp::ok(json!({
        "list": list, "total": total, "page": q.page, "pageSize": q.page_size,
    }))))
}

/// CR 详情(query: ?crId=,返回头+行)
pub async fn mdm_cr_detail(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    headers: HeaderMap,
    Query(q): Query<CrDetailQuery>,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_pg_db_manager();
    let db_id = resolve_db_id_from_headers(&headers).await;
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

/// CR 列表查询（分页）。
#[derive(serde::Deserialize)]
pub struct CrListQuery {
    #[serde(default, alias = "docStatus")]
    pub doc_status: Option<String>,
    #[serde(default = "default_page")]
    pub page: i64,
    #[serde(default = "default_page_size", alias = "pageSize")]
    pub page_size: i64,
    /// 是否返回 payload（列表默认 false 不查 payload，影响效率）
    #[serde(default, alias = "withPayload")]
    pub with_payload: bool,
}
fn default_page() -> i64 { 1 }
fn default_page_size() -> i64 { 20 }

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
/// 仅用于 merge/undo 的明细表 reparent（头表名现由 body.targetTable 传入）。
fn dict_tables(dict_code: &str) -> Option<(String, Vec<(String, String)>)> {
    match dict_code {
        "supplier" => Some((
            "cm_supplier".into(),
            vec![("cm_bank_account".into(), "supplier_id".into())],
        )),
        _ => None,
    }
}

fn load_columns() -> Vec<&'static str> {
    vec!["id", "name", "tax_no", "credit_code", "short_name", "phone", "update_time"]
}

/// 实时查重（纯查询，不落库）。body { dictCode, recordId, targetTable, specs, clusterKeys, surviveFields }。
///
/// 返回目标记录字段值 + 每个候选的字段值（供前端做字段对比表）。
/// 候选裁决：≥95 自动合并 / 80-94 待评审 / <80 不匹配（[match_algo::decide] 双阈值）。
pub async fn mdm_find_duplicates(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(body): Json<FindDupBody>,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_pg_db_manager();
    let db_id = resolve_db_id_from_headers(&headers).await;

    // 把 DTO specs 转成 MatchFieldSpec（校验 kind 合法）
    let specs: Vec<MatchFieldSpec> = body.specs.iter()
        .map(|s| s.to_match_spec()).collect::<Result<Vec<_>>>()?;
    if specs.is_empty() {
        return Err(store::api_err("查重字段（specs）不能为空"));
    }
    let cluster_keys: Vec<&str> = body.cluster_keys.iter().map(|s| s.as_str()).collect();

    // 装载列 = id ∪ specs 字段 ∪ surviveFields ∪ displayFields ∪ {update_time}
    // （防注入经 load_published validate_ident；displayFields 仅展示用，如 label/code）
    let mut col_set: Vec<String> = vec!["id".into(), "update_time".into()];
    for s in &body.specs { col_set.push(s.field.clone()); }
    for f in &body.survive_fields { col_set.push(f.clone()); }
    for f in &body.display_fields { col_set.push(f.clone()); }
    col_set.sort(); col_set.dedup();
    let columns: Vec<&str> = col_set.iter().map(|s| s.as_str()).collect();

    let all = store::load_published(mm, &db_id, &body.target_table, &columns).await?;
    let target = all
        .iter()
        .find(|r| r.id == body.record_id)
        .cloned()
        .ok_or_else(|| store::api_err(&format!("记录 {} 不存在或非 published", body.record_id)))?;

    let candidates = find_candidates(&target, &all, &specs, &cluster_keys);

    // 不落库（查重预览）。落库收敛到 mdm_merge_requests_create 一处。
    Ok(Json(ApiResp::ok(json!({
        "targetId": target.id,
        "targetFields": target.fields,
        "candidates": candidates.iter().map(|c| {
            // 回填候选的字段值（供前端对比表）
            let rec = all.iter().find(|r| r.id == c.record_id);
            json!({
                "recordId": c.record_id,
                "score": c.score,
                "decision": format!("{:?}", c.decision),
                "fields": rec.map(|r| r.fields.clone()).unwrap_or_default(),
            })
        }).collect::<Vec<_>>(),
        "thresholds": { "auto_merge": 95, "review": 80 },
    }))))
}

/// 关键信息查重（V3.2 步骤条预校验：新建场景，无 recordId）。
///
/// 与 `mdm_find_duplicates` 的区别：find-duplicates 需 recordId（从已发布记录查重）；
/// check-key 是**新建场景**，用前端提交的关键信息构造虚拟 target（id=0），与激活区已发布记录比对。
/// 命中（score ≥ 80）即视为重复，前端弹框阻断，不允许进入步骤2。
///
/// body: { dictCode, targetTable, keyValue, specs, clusterKeys }
/// 返回: { exists: false } 或 { exists: true, id, code, message }
pub async fn mdm_check_key(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(body): Json<CheckKeyBody>,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_pg_db_manager();
    let db_id = resolve_db_id_from_headers(&headers).await;

    // specs → MatchFieldSpec（校验 kind 合法）
    let specs: Vec<MatchFieldSpec> = body
        .specs
        .iter()
        .map(|s| s.to_match_spec())
        .collect::<Result<Vec<_>>>()?;
    if specs.is_empty() {
        return Err(store::api_err("查重字段（specs）不能为空"));
    }
    let cluster_keys: Vec<&str> = body.cluster_keys.iter().map(|s| s.as_str()).collect();

    // 装载列 = id ∪ specs 字段 ∪ {code, name, update_time}（code/name 用于返回给前端展示）
    let mut col_set: Vec<String> = vec!["id".into(), "code".into(), "name".into(), "update_time".into()];
    for s in &body.specs {
        col_set.push(s.field.clone());
    }
    col_set.sort();
    col_set.dedup();
    let columns: Vec<&str> = col_set.iter().map(|s| s.as_str()).collect();

    // 拉激活区全量已发布记录
    let all = store::load_published(mm, &db_id, &body.target_table, &columns).await?;

    // 构造虚拟 target：id=0（表示未落库），fields = keyValue
    use cmx_mdm_model::match_algo::MatchRecord;
    let target = MatchRecord {
        id: 0,
        fields: body.key_value.clone(),
    };

    let candidates = find_candidates(&target, &all, &specs, &cluster_keys);

    // 命中即阻断（score ≥ 80 = Review 阈值）
    if let Some(first) = candidates.first() {
        // 找到匹配记录，取 id/code 用于返回
        let rec = all.iter().find(|r| r.id == first.record_id);
        let code = rec
            .and_then(|r| r.fields.get("code"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let name = rec
            .and_then(|r| r.fields.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        // 拼展示消息：已存在相同记录：SUP001（A公司）
        let display = match (code, name) {
            (c, n) if !c.is_empty() && !n.is_empty() => format!("{}（{}）", c, n),
            (c, "") if !c.is_empty() => c.to_string(),
            ("", n) if !n.is_empty() => n.to_string(),
            _ => format!("id={}", first.record_id),
        };
        return Ok(Json(ApiResp::ok(json!({
            "exists": true,
            "id": first.record_id,
            "code": code,
            "message": format!("已存在相同记录：{}", display),
        }))));
    }

    Ok(Json(ApiResp::ok(json!({ "exists": false }))))
}

/// check-key 请求体（V3.2 步骤条预校验）。
#[derive(serde::Deserialize)]
pub struct CheckKeyBody {
    #[serde(alias = "dictCode")]
    pub dict_code: String,
    #[serde(alias = "targetTable")]
    pub target_table: String,
    /// 关键信息字段值（虚拟 target 的 fields），如 { "name": "A公司", "tax_no": "911..." }
    #[serde(alias = "keyValue")]
    pub key_value: serde_json::Map<String, Value>,
    /// 比较字段规则（同 FindDupBody.specs）
    #[serde(default)]
    pub specs: Vec<SpecDto>,
    /// 分块簇键（同 FindDupBody.cluster_keys）
    #[serde(default, alias = "clusterKeys")]
    pub cluster_keys: Vec<String>,
}


/// 默认排除 pending（查重预览不再落 pending；历史区只看真正合并过的）。
pub async fn mdm_merge_requests_list(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    headers: HeaderMap,
    Query(q): Query<MergeListQuery>,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_pg_db_manager();
    let db_id = resolve_db_id_from_headers(&headers).await;
    // excludePending 默认 true（"1"/"true"）；显式传 false 关闭
    let exclude_pending = match q.exclude_pending.as_deref() {
        Some("0") | Some("false") | Some("False") => false,
        _ => true,
    };
    let exclude_statuses: Option<&[&str]> = if exclude_pending { Some(&["pending"]) } else { None };
    let (list, total) = store::list_match_groups(
        mm, &db_id, q.dict_code.as_deref(), q.status.as_deref(),
        exclude_statuses, q.page, q.page_size)
        .await?;

    // 回填可读名称：按 group 的 master_id / member_ids 联查目标表 name/code
    let list = enrich_group_names(&mm, &db_id, list).await;

    Ok(Json(ApiResp::ok(json!({
        "list": list, "total": total, "page": q.page, "pageSize": q.page_size,
    }))))
}

/// 回填每条 match_group 的 master/member 可读名称。
/// member_ids 是 JSONB（DB 返回转义字符串），parse 后联查目标表。
async fn enrich_group_names(
    mm: &DatabaseManager,
    db_id: &str,
    mut groups: Vec<Value>,
) -> Vec<Value> {
    // 按 dict_code 分组批量查（每字典一次 load_by_ids）
    use std::collections::HashMap;
    let mut by_dict: HashMap<String, Vec<i64>> = HashMap::new();
    for g in &groups {
        let dict_code = g.get("dict_code").and_then(|v| v.as_str()).unwrap_or("").to_string();
        if dict_code.is_empty() { continue; }
        let master_id = g.get("master_id").and_then(|v| v.as_i64()).unwrap_or(0);
        let members_raw = g.get("member_ids").cloned().unwrap_or(Value::Null);
        let members = match members_raw {
            Value::String(s) => serde_json::from_str::<Value>(&s).unwrap_or(Value::Null),
            v => v,
        };
        let member_ids: Vec<i64> = members
            .as_array()
            .map(|a| a.iter().filter_map(|m| m.as_i64()).collect())
            .unwrap_or_default();
        let entry = by_dict.entry(dict_code).or_default();
        if master_id > 0 { entry.push(master_id); }
        for id in member_ids { entry.push(id); }
    }

    // 每字典查一次（dict→table 映射仍用注册表；通用化后 target_table 由配置带，此处兜底 supplier）
    let mut name_cache: HashMap<(String, i64), (String, String)> = HashMap::new(); // (dict,id) -> (name,code)
    for (dict_code, ids) in &by_dict {
        let table = match dict_code.as_str() {
            "supplier" => "cm_supplier",
            _ => continue, // 未知字典跳过（名称留空）
        };
        let cols = ["id", "name", "code"];
        if let Ok(rows) = store::load_by_ids(mm, db_id, None, table, &cols, ids).await {
            for r in rows {
                let get = |k: &str| r.fields.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
                name_cache.insert((dict_code.clone(), r.id), (get("name"), get("code")));
            }
        }
    }

    for g in groups.iter_mut() {
        let dict_code = g.get("dict_code").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let master_id = g.get("master_id").and_then(|v| v.as_i64()).unwrap_or(0);
        if let Some((n, c)) = name_cache.get(&(dict_code.clone(), master_id)) {
            g["masterName"] = json!(n);
            g["masterCode"] = json!(c);
        }
        let members_raw = g.get("member_ids").cloned().unwrap_or(Value::Null);
        let members = match members_raw {
            Value::String(s) => serde_json::from_str::<Value>(&s).unwrap_or(Value::Null),
            v => v,
        };
        let member_names: Vec<Value> = members
            .as_array()
            .map(|a| {
                a.iter().filter_map(|m| m.as_i64()).map(|id| {
                    let (n, c) = name_cache.get(&(dict_code.clone(), id)).cloned().unwrap_or_default();
                    json!({ "id": id, "name": n, "code": c })
                }).collect()
            })
            .unwrap_or_default();
        g["memberNames"] = json!(member_names);
    }
    groups
}

/// 确认合并。body { dictCode, masterId, victimIds, survivorship? }。
pub async fn mdm_merge_requests_create(
    State(_s): State<CmxAppState>,
    svr_ctx: CmxSvrContext,
    headers: HeaderMap,
    Json(body): Json<MergeBody>,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_pg_db_manager();
    let db_id = resolve_db_id_from_headers(&headers).await;
    // 头表名由 body.targetTable 传入（来自查重规则，替代硬编码 dict_tables 头表）；
    // line_tables（明细表 reparent）仍由 dict_tables 解析，未知字典给空明细。
    let head_table = body.target_table.clone();
    let line_tables: Vec<(String, String)> = dict_tables(&body.dict_code)
        .map(|(_h, lines)| lines)
        .unwrap_or_default();
    let operated_by = actor_id_i64(&svr_ctx);

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

    // 存活字段由 body.survive_fields 传入（来自查重规则）；空则 master 原值全保留
    let survive_fields: Vec<String> = body.survive_fields.clone();
    let master_id = store::merge(
        mm, &db_id, &body.dict_code, &head_table, body.master_id, &body.victim_ids,
        &survive_fields, &rules, &overrides, &line_tables, operated_by, group_id,
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
    let db_id = resolve_db_id_from_headers(&headers).await;
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
    svr_ctx: CmxSvrContext,
    headers: HeaderMap,
    Json(body): Json<RejectMergeBody>,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_pg_db_manager();
    let db_id = resolve_db_id_from_headers(&headers).await;
    let operated_by = actor_id_i64(&svr_ctx);
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
    svr_ctx: CmxSvrContext,
    headers: HeaderMap,
    Json(body): Json<UndoBody>,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_pg_db_manager();
    let db_id = resolve_db_id_from_headers(&headers).await;
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
    let operated_by = actor_id_i64(&svr_ctx);

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
    /// 目标头物理表（从 dct/meta tableName 或 match_config 带入，替代硬编码 dict_tables）
    #[serde(alias = "targetTable")]
    pub target_table: String,
    /// 比较字段规则（替代硬编码 default_specs）
    #[serde(default)]
    pub specs: Vec<SpecDto>,
    /// 分块簇键（替代硬编码 default_cluster_keys）
    #[serde(default, alias = "clusterKeys")]
    pub cluster_keys: Vec<String>,
    /// 存活字段（供前端做字段对比展示用；查重本身只需 specs）
    #[serde(default, alias = "surviveFields")]
    pub survive_fields: Vec<String>,
    /// 仅用于展示的附加列（如 labelField/codeField），不参与匹配/存活，只随候选字段返回
    #[serde(default, alias = "displayFields")]
    pub display_fields: Vec<String>,
}

/// 比较字段 DTO（kind: "Exact" | "EditDistance"）。
#[derive(serde::Deserialize)]
pub struct SpecDto {
    pub field: String,
    #[serde(default = "default_weight")]
    pub weight: u32,
    #[serde(default = "default_kind")]
    pub kind: String,
}
fn default_weight() -> u32 { 0 }
fn default_kind() -> String { "Exact".into() }

impl SpecDto {
    fn to_match_spec(&self) -> Result<MatchFieldSpec> {
        let kind = match self.kind.as_str() {
            "Exact" | "exact" => FieldKind::Exact,
            "EditDistance" | "edit_distance" | "editDistance" => FieldKind::EditDistance,
            other => return Err(store::api_err(&format!(
                "字段 {field} 的比较方式 {other:?} 不合法（Exact / EditDistance）", field = self.field
            ))),
        };
        Ok(MatchFieldSpec { field: self.field.clone(), weight: self.weight, kind })
    }
}

#[derive(serde::Deserialize)]
pub struct MergeListQuery {
    #[serde(default, alias = "dictCode")]
    pub dict_code: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    /// 默认排除 pending（查重预览不再落 pending；历史区只看真正合并过的）。
    /// "1"/"true" 或缺省=排除；"0"/"false"=不排除。
    #[serde(default, alias = "excludePending")]
    pub exclude_pending: Option<String>,
    #[serde(default = "default_page")]
    pub page: i64,
    #[serde(default = "default_page_size", alias = "pageSize")]
    pub page_size: i64,
}

/// 审计/事件/订阅 列表查询（分页，无 path variable）。
#[derive(serde::Deserialize)]
pub struct GovListQuery {
    #[serde(default, alias = "dictCode")]
    pub dict_code: Option<String>,
    #[serde(default, alias = "recordId")]
    pub record_id: Option<i64>,
    #[serde(default)]
    pub since: Option<i64>,
    #[serde(default = "default_page")]
    pub page: i64,
    #[serde(default = "default_page_size", alias = "pageSize")]
    pub page_size: i64,
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
    /// 目标头物理表（来自查重规则，替代硬编码 dict_tables 头表）
    #[serde(default, alias = "targetTable")]
    pub target_table: String,
    /// 存活字段（来自查重规则，替代硬编码 default_survive_fields）
    #[serde(default, alias = "surviveFields")]
    pub survive_fields: Vec<String>,
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

// ════════════════════════════════════════════════════════════════════════════
// MDM 治理端点（分页 + 无 path variable，参数走 query/body）
// ════════════════════════════════════════════════════════════════════════════

/// 变更历史/版本留痕。GET ?dictCode=&recordId=&page=&pageSize=。
pub async fn mdm_audit_list(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    headers: HeaderMap,
    Query(q): Query<GovListQuery>,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_pg_db_manager();
    let db_id = resolve_db_id_from_headers(&headers).await;
    let (list, total) = store::list_audit(
        mm, &db_id, q.dict_code.as_deref(), q.record_id, q.page, q.page_size).await?;
    Ok(Json(ApiResp::ok(json!({ "list": list, "total": total, "page": q.page, "pageSize": q.page_size }))))
}

/// 事件查询（delta）。GET ?dictCode=&since=&page=&pageSize=。
pub async fn mdm_events_list(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    headers: HeaderMap,
    Query(q): Query<GovListQuery>,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_pg_db_manager();
    let db_id = resolve_db_id_from_headers(&headers).await;
    let (list, total) = store::list_events(
        mm, &db_id, q.dict_code.as_deref(), q.since, q.page, q.page_size).await?;
    Ok(Json(ApiResp::ok(json!({ "list": list, "total": total, "page": q.page, "pageSize": q.page_size }))))
}

/// 订阅配置列表。GET ?page=&pageSize=。
pub async fn mdm_subscriptions_list(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    headers: HeaderMap,
    Query(q): Query<GovListQuery>,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_pg_db_manager();
    let db_id = resolve_db_id_from_headers(&headers).await;
    let (list, total) = store::list_subscriptions(mm, &db_id, q.page, q.page_size).await?;
    Ok(Json(ApiResp::ok(json!({ "list": list, "total": total, "page": q.page, "pageSize": q.page_size }))))
}

/// 订阅配置保存。POST body { id?, target_sys, dict_code, channel, active, filter?, field_map? }。
pub async fn mdm_subscriptions_save(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_pg_db_manager();
    let db_id = resolve_db_id_from_headers(&headers).await;
    let id = store::upsert_subscription(mm, &db_id, &body).await?;
    Ok(Json(ApiResp::ok(json!({ "id": id }))))
}

/// 发布。POST body { dict }（M5 分发前置；当前写一条 publish 事件占位）。
pub async fn mdm_publish(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<ApiResp<Value>>> {
    let _headers = headers;
    let dict = body.get("dict").and_then(|v| v.as_str()).unwrap_or("");
    if dict.is_empty() { return Err(store::api_err("dict 不能为空")) }
    Ok(Json(ApiResp::ok(json!({ "dict": dict, "published": true }))))
}

// ════════════════════════════════════════════════════════════════════════════
// 查重规则配置（match-configs）—— 规则维护内嵌查重界面，无独立管理页
// ════════════════════════════════════════════════════════════════════════════

/// 查重规则列表。GET ?dictCode=（可空，空则列全部）。
pub async fn mdm_match_configs_list(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    headers: HeaderMap,
    Query(q): Query<MatchConfigQuery>,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_pg_db_manager();
    let db_id = resolve_db_id_from_headers(&headers).await;
    let list = store::list_match_config(mm, &db_id, q.dict_code.as_deref()).await?;
    Ok(Json(ApiResp::ok(json!(list))))
}

/// 查重规则保存（upsert）。POST body { id?, ruleName, dictCode, targetTable, specs, clusterKeys, surviveFields, thresholds? }。
/// id 缺省/0=新建；id 非零或 (dictCode,ruleName) 已存在=更新。返回规则 id（i64）。
pub async fn mdm_match_configs_save(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_pg_db_manager();
    let db_id = resolve_db_id_from_headers(&headers).await;
    let id = store::upsert_match_config(mm, &db_id, &body).await?;
    Ok(Json(ApiResp::ok(json!({ "id": id }))))
}

/// 查重规则删除（软删 is_active=FALSE）。POST body { configId }。
pub async fn mdm_match_configs_delete(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(body): Json<MatchConfigDeleteBody>,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_pg_db_manager();
    let db_id = resolve_db_id_from_headers(&headers).await;
    let n = store::delete_match_config(mm, &db_id, body.config_id).await?;
    Ok(Json(ApiResp::ok(json!({ "configId": body.config_id, "affected": n }))))
}

#[derive(serde::Deserialize)]
pub struct MatchConfigQuery {
    #[serde(default, alias = "dictCode")]
    pub dict_code: Option<String>,
}

#[derive(serde::Deserialize)]
pub struct MatchConfigDeleteBody {
    #[serde(alias = "configId")]
    pub config_id: i64,
}
