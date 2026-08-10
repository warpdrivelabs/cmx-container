//! 实时查重 + 关键信息查重 handler。
//!
//! 对应路由（`cmx-mdm-api/src/lib.rs`）：
//! - `POST /mdm/records/find-duplicates` → [`mdm_find_duplicates`]
//! - `POST /mdm/check-key` → [`mdm_check_key`]
//!
//! 本模块还提供 [`dict_tables`] / [`load_columns`] 两个注册表辅助函数，
//! 供 [`super::merge`] 的明细表 reparent / 详情列装载复用。

use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use serde_json::{json, Value};

use cmx_api::CmxAppState;
use cmx_api::db_id::resolve_db_id_from_headers;
use cmx_api::middleware::CmxSvrContext;
use cmx_api::{ApiResp, Result};

use cmx_database_pg::get_default_pg_db_manager;
use cmx_mdm_model::match_algo::{find_candidates, MatchRecord};
use cmx_mdm_store_pg as store;

use super::SpecDto;

/// dict → 物理表 / 明细表解析（M3 MVP 注册表；M6 多域改走 DCT meta tableName）。
///
/// 仅用于 merge/undo 的明细表 reparent（头表名现由 body.targetTable 传入）。
///
/// # Returns
///
/// `Some((head_table, line_tables))` 当字典已注册；`None` 当未知字典。
/// `line_tables` 元素为 `(明细表名, 外键列名)`。
pub(crate) fn dict_tables(dict_code: &str) -> Option<(String, Vec<(String, String)>)> {
    match dict_code {
        "supplier" => Some((
            "cm_supplier".into(),
            vec![("cm_bank_account".into(), "supplier_id".into())],
        )),
        _ => None,
    }
}

/// 详情列装载白名单（load_by_ids 的列集）。
pub(crate) fn load_columns() -> Vec<&'static str> {
    vec![
        "id",
        "name",
        "tax_no",
        "credit_code",
        "short_name",
        "phone",
        "update_time",
    ]
}

/// 实时查重（纯查询，不落库）。body `{ dictCode, recordId, targetTable, specs, clusterKeys, surviveFields }`。
///
/// 返回目标记录字段值 + 每个候选的字段值（供前端做字段对比表）。
/// 候选裁决：≥95 自动合并 / 80-94 待评审 / <80 不匹配（[`cmx_mdm_model::match_algo::decide`] 双阈值）。
pub async fn mdm_find_duplicates(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(body): Json<FindDupBody>,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_pg_db_manager();
    let db_id = resolve_db_id_from_headers(&headers).await;

    // 把 DTO specs 转成 MatchFieldSpec（校验 kind 合法）
    let specs: Vec<_> = body
        .specs
        .iter()
        .map(|s| s.to_match_spec())
        .collect::<Result<Vec<_>>>()?;
    if specs.is_empty() {
        return Err(store::api_err("查重字段（specs）不能为空"));
    }
    let cluster_keys: Vec<&str> = body.cluster_keys.iter().map(|s| s.as_str()).collect();

    // 装载列 = id ∪ specs 字段 ∪ surviveFields ∪ displayFields ∪ {update_time}
    // （防注入经 load_published validate_ident；displayFields 仅展示用，如 label/code）
    let mut col_set: Vec<String> = vec!["id".into(), "update_time".into()];
    for s in &body.specs {
        col_set.push(s.field.clone());
    }
    for f in &body.survive_fields {
        col_set.push(f.clone());
    }
    for f in &body.display_fields {
        col_set.push(f.clone());
    }
    col_set.sort();
    col_set.dedup();
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
/// 与 [`mdm_find_duplicates`] 的区别：find-duplicates 需 recordId（从已发布记录查重）；
/// check-key 是**新建场景**，用前端提交的关键信息构造虚拟 target（id=0），与激活区已发布记录比对。
/// 命中（score ≥ 80）即视为重复，前端弹框阻断，不允许进入步骤2。
///
/// body: `{ dictCode, targetTable, keyValue, specs, clusterKeys }`
///
/// 返回: `{ exists: false }` 或 `{ exists: true, id, code, message }`。
pub async fn mdm_check_key(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(body): Json<CheckKeyBody>,
) -> Result<Json<ApiResp<Value>>> {
    let mm = get_default_pg_db_manager();
    let db_id = resolve_db_id_from_headers(&headers).await;

    // specs → MatchFieldSpec（校验 kind 合法）
    let specs: Vec<_> = body
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

/// find-duplicates 请求体。
#[derive(serde::Deserialize)]
pub struct FindDupBody {
    #[serde(alias = "dictCode")]
    pub dict_code: String,
    #[serde(alias = "recordId")]
    pub record_id: i64,
    /// 目标头物理表（从 dct/meta tableName 或 match_config 带入，替代硬编码 dict_tables）。
    #[serde(alias = "targetTable")]
    pub target_table: String,
    /// 比较字段规则（替代硬编码 default_specs）。
    #[serde(default)]
    pub specs: Vec<SpecDto>,
    /// 分块簇键（替代硬编码 default_cluster_keys）。
    #[serde(default, alias = "clusterKeys")]
    pub cluster_keys: Vec<String>,
    /// 存活字段（供前端做字段对比展示用；查重本身只需 specs）。
    #[serde(default, alias = "surviveFields")]
    pub survive_fields: Vec<String>,
    /// 仅用于展示的附加列（如 labelField/codeField），不参与匹配/存活，只随候选字段返回。
    #[serde(default, alias = "displayFields")]
    pub display_fields: Vec<String>,
}

/// check-key 请求体（V3.2 步骤条预校验）。
#[derive(serde::Deserialize)]
pub struct CheckKeyBody {
    #[serde(alias = "dictCode")]
    pub dict_code: String,
    #[serde(alias = "targetTable")]
    pub target_table: String,
    /// 关键信息字段值（虚拟 target 的 fields），如 `{ "name": "A公司", "tax_no": "911..." }`。
    #[serde(alias = "keyValue")]
    pub key_value: serde_json::Map<String, Value>,
    /// 比较字段规则（同 [`FindDupBody::specs`]）。
    #[serde(default)]
    pub specs: Vec<SpecDto>,
    /// 分块簇键（同 [`FindDupBody::cluster_keys`]）。
    #[serde(default, alias = "clusterKeys")]
    pub cluster_keys: Vec<String>,
}
