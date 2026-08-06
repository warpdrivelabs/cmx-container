//! 编码引擎 HTTP handlers。
//!
//! 按 dct_upsert 模板：State + CmxSvrContext + Query/Json，返回 ApiResp<Value>。

use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use cmx_api::middleware::CmxSvrContext;
use cmx_api::{ApiResp, CmxAppState};
use cmx_code_model::context::ResolveContext;
use cmx_code_model::spec::{RuleSpec, Target};
// db_id 兜底（header 优先 → 否则第一个 biz 库 → 再退 default）
use cmx_database_pg::get_default_pg_db_manager;

use crate::{engine, store::{gap_store, rule_store}};

// ═══════════════════════════════════════════════════════════════════════════════
// 辅助
// ═══════════════════════════════════════════════════════════════════════════════

/// 从请求头取 db_id（与 DCT resolve_db_id 同款兜底）。
///
/// 优先级：`db_id` 请求头 → 第一个 `source_type="biz"` 的业务库 → 默认库。
/// 这样前端不带 db_id 时，规则自动落到业务库（如 fico-db），与字典数据同库。
async fn db_id_from(headers: &HeaderMap) -> String {
    if let Some(v) = headers.get("db_id").and_then(|h| h.to_str().ok()) {
        let s = v.trim();
        if !s.is_empty() {
            return s.to_string();
        }
    }
    get_default_pg_db_manager().get_biz_db_id().await
}

/// 域/应用/模块三维标识（从请求头取，前端规则管理页会带）。
#[derive(Debug, Clone, Default)]
pub struct Dam {
    pub domain_code: String,
    pub application_code: String,
    pub module_code: String,
}

/// 从请求头取 DAM（domain_code/application_code/module_code）。
fn dam_from(headers: &HeaderMap) -> Dam {
    let read = |key: &str| -> String {
        headers
            .get(key)
            .and_then(|h| h.to_str().ok())
            .unwrap_or("")
            .to_string()
    };
    Dam {
        domain_code: read("domain_code"),
        application_code: read("application_code"),
        module_code: read("module_code"),
    }
}

/// 错误转 ApiResp（按错误类型分 status code：参数错 400 / 未找到 404 / 唯一冲突 409 / 内部错 500）。
///
/// 唯一冲突识别：`CodeError::Database(msg)` 的 `msg` 经 store 层 `pg_detail` 已含
/// `duplicate key value violates unique constraint` 等稳定子串（对齐
/// `cmx_biz::errcode::classify_db_error` 逻辑），命中则返 409 + 中文友好提示，
/// 并从 PG DETAIL `Key (rule_code)=(XXX) already exists` 抽出具体编码。
fn err_resp(e: cmx_code_model::error::CodeError) -> Json<ApiResp<Value>> {
    use cmx_code_model::error::CodeError;
    let (status, msg) = match &e {
        CodeError::InvalidSegment(_) | CodeError::PatternMismatch { .. } => {
            (400u16, e.to_string())
        }
        CodeError::NoMatchingRule(_) | CodeError::RefFieldMissing(_) => {
            (404u16, e.to_string())
        }
        CodeError::Database(detail) => {
            let lower = detail.to_ascii_lowercase();
            if lower.contains("duplicate key") || lower.contains("unique constraint") {
                // 从 PG DETAIL「Key (rule_code)=(DOC_NO) already exists」抽 rule_code
                let code = extract_duplicate_key(detail);
                let friendly = match code {
                    Some(c) => format!("规则编码「{c}」已存在，请更换后再试"),
                    None => "规则编码已存在，请更换后再试".to_string(),
                };
                (409u16, friendly)
            } else {
                (500u16, e.to_string())
            }
        }
        _ => (500u16, e.to_string()),
    };
    Json(ApiResp::fail(status, &msg))
}

/// 从 PG 唯一冲突 DETAIL 文本中抽取冲突列的值。
///
/// PG 典型形态：`Key (rule_code)=(DOC_NO) already exists.` → 返回 `Some("DOC_NO")`。
/// 抽不到返回 None（调用方用通用提示）。
fn extract_duplicate_key(detail: &str) -> Option<String> {
    // 匹配 `Key (col)=(value) already exists` 中的 value
    let marker = ") already exists";
    let pos = detail.to_ascii_lowercase().find(marker)?;
    let head = &detail[..pos];
    // 末尾的 `(value)` —— 反向找最近一个 `=(`
    let eq_pos = head.rfind("=(")?;
    let val = &head[eq_pos + 2..];
    let val = val.trim().trim_matches('(').trim_matches(')');
    let val = val.trim();
    if val.is_empty() {
        None
    } else {
        Some(val.to_string())
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 规则库 CRUD
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Deserialize)]
pub struct RuleListQuery {
    #[serde(default)]
    pub fields: Option<String>,
}

/// GET /api/code/rules —— 列出规则（ruleCode + ruleName，供下拉源用）。
///
/// 按请求头 DAM 过滤：带了 domain_code 时只返回该模块的规则，不带时返回全部。
pub async fn rule_list(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    headers: HeaderMap,
    Query(_q): Query<RuleListQuery>,
) -> Json<ApiResp<Value>> {
    let db_id = db_id_from(&headers).await;
    let dam = dam_from(&headers);
    match rule_store::list_rules(&db_id, &dam).await {
        Ok(rules) => Json(ApiResp::ok(json!({ "rules": rules }))),
        Err(e) => err_resp(e),
    }
}

/// POST /api/code/rules —— 创建规则。
pub async fn rule_create(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    headers: HeaderMap,
    mut rule: Json<RuleSpec>,
) -> Json<ApiResp<Value>> {
    let db_id = db_id_from(&headers).await;
    // 请求头 DAM 补进 rule（前端 body 未带时，以请求头所在模块为准）
    let dam = dam_from(&headers);
    if rule.domain_code.is_empty() {
        rule.domain_code = dam.domain_code.clone();
    }
    if rule.application_code.is_empty() {
        rule.application_code = dam.application_code.clone();
    }
    if rule.module_code.is_empty() {
        rule.module_code = dam.module_code.clone();
    }
    match rule_store::create_rule(&rule, &db_id).await {
        Ok(_) => Json(ApiResp::ok(json!({ "ruleCode": rule.rule_code }))),
        Err(e) => err_resp(e),
    }
}

/// GET /api/code/rules/{ruleCode} —— 单条规则。
pub async fn rule_get(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    headers: HeaderMap,
    Path(rule_code): Path<String>,
) -> Json<ApiResp<Value>> {
    let db_id = db_id_from(&headers).await;
    let dam = dam_from(&headers);
    match rule_store::get_rule(&rule_code, &db_id, &dam).await {
        Ok(rule) => Json(ApiResp::ok(json!(rule))),
        Err(e) => err_resp(e),
    }
}

/// PUT /api/code/rules/{ruleCode} —— 更新规则。
pub async fn rule_update(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    headers: HeaderMap,
    Path(rule_code): Path<String>,
    mut rule: Json<RuleSpec>,
) -> Json<ApiResp<Value>> {
    let db_id = db_id_from(&headers).await;
    // 请求头 DAM 补进 rule（防止前端漏传导致规则脱离模块）
    let dam = dam_from(&headers);
    if rule.domain_code.is_empty() {
        rule.domain_code = dam.domain_code.clone();
    }
    if rule.application_code.is_empty() {
        rule.application_code = dam.application_code.clone();
    }
    if rule.module_code.is_empty() {
        rule.module_code = dam.module_code.clone();
    }
    match rule_store::update_rule(&rule_code, &rule, &db_id).await {
        Ok(_) => Json(ApiResp::ok(json!({ "ruleCode": rule_code }))),
        Err(e) => err_resp(e),
    }
}

/// DELETE /api/code/rules/{ruleCode} —— 删除规则（软删除）。
pub async fn rule_delete(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    headers: HeaderMap,
    Path(rule_code): Path<String>,
) -> Json<ApiResp<Value>> {
    let db_id = db_id_from(&headers).await;
    let dam = dam_from(&headers);
    match rule_store::delete_rule(&rule_code, &db_id, &dam).await {
        Ok(_) => Json(ApiResp::ok(json!({ "ruleCode": rule_code, "deleted": true }))),
        Err(e) => err_resp(e),
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 预览 / 生成 / 校验
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Deserialize)]
pub struct PreviewBody {
    pub target: Target,
    #[serde(default)]
    pub attrs: Value,
    #[serde(default)]
    pub rule_code: Option<String>,
}

/// 批量预览/生成 body（方案 §11 preview/batch + generate/batch）。
#[derive(Deserialize)]
pub struct BatchBody {
    pub target: Target,
    #[serde(default)]
    pub rows: Vec<Value>,
    #[serde(default)]
    pub rule_code: Option<String>,
}

/// POST /api/code/preview —— 预览编码（不落库不占号）。
pub async fn preview(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(body): Json<PreviewBody>,
) -> Json<ApiResp<Value>> {
    let db_id = db_id_from(&headers).await;
    let rule_code = match &body.rule_code {
        Some(rc) => rc.clone(),
        None => {
            return Json(ApiResp::fail(
                400u16,
                "预览请求缺 ruleCode",
            ))
        }
    };

    let rule = match rule_store::get_rule(&rule_code, &db_id, &Dam::default()).await {
        Ok(r) => r,
        Err(e) => return err_resp(e),
    };

    let ctx = ResolveContext::new(&db_id, None).with(body.attrs);
    let advance = engine::pg_advance(&db_id, None);

    match engine::preview(&rule, &body.target, &ctx, &advance).await {
        Ok(code) => Json(ApiResp::ok(json!({
            "code": code,
            "ruleCode": rule_code,
            "warning": "预览码非定稿，最终以保存时为准"
        }))),
        Err(e) => err_resp(e),
    }
}

/// 批量铸号共用逻辑（preview_batch / generate_batch 复用）。
async fn run_batch(
    body: BatchBody,
    db_id: &str,
    label: &str,
    with_warning: bool,
) -> Json<ApiResp<Value>> {
    let rule_code = match &body.rule_code {
        Some(rc) => rc.clone(),
        None => return Json(ApiResp::fail(400u16, &format!("批量{label}请求缺 ruleCode"))),
    };
    if body.rows.is_empty() {
        return Json(ApiResp::ok(json!({ "codes": [], "warning": "rows 为空" })));
    }
    let code_rule = serde_json::json!({ "ruleCode": rule_code, "mode": "auto", "field": body.target.field });
    let target_val = serde_json::to_value(&body.target).unwrap_or_default();
    let minter = crate::engine::CodeEngine;
    match <crate::engine::CodeEngine as cmx_traits::code::CodeMinter>::mint_batch(
        &minter, &code_rule, &target_val, &body.rows, db_id, None,
    ).await {
        Ok(codes) => {
            let mut data = json!({ "codes": codes, "ruleCode": rule_code });
            if with_warning {
                data["warning"] = json!("预览码非定稿，最终以保存时为准");
            }
            Json(ApiResp::ok(data))
        }
        Err(e) => Json(ApiResp::fail(500u16, &format!("批量{label}失败：{e}"))),
    }
}

/// POST /api/code/preview/batch —— 批量预览（N 行同表，不落库不占号）。
pub async fn preview_batch(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(body): Json<BatchBody>,
) -> Json<ApiResp<Value>> {
    let db_id = db_id_from(&headers).await;
    run_batch(body, &db_id, "预览", true).await
}

/// POST /api/code/generate —— 权威生成（事务内铸号）。
pub async fn generate(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(body): Json<PreviewBody>,
) -> Json<ApiResp<Value>> {
    let db_id = db_id_from(&headers).await;
    let rule_code = match &body.rule_code {
        Some(rc) => rc.clone(),
        None => {
            return Json(ApiResp::fail(
                400u16,
                "生成请求缺 ruleCode",
            ))
        }
    };

    let rule = match rule_store::get_rule(&rule_code, &db_id, &Dam::default()).await {
        Ok(r) => r,
        Err(e) => return err_resp(e),
    };

    let ctx = ResolveContext::new(&db_id, None).with(body.attrs);
    let advance = engine::pg_advance(&db_id, None);

    match engine::mint(&rule, &body.target, &ctx, &advance).await {
        Ok(code) => Json(ApiResp::ok(json!({ "code": code, "ruleCode": rule_code }))),
        Err(e) => err_resp(e),
    }
}

/// POST /api/code/generate/batch —— 批量生成（N 行同表）。
///
/// 与 preview_batch 同走 `CodeEngine::mint_batch`（算号不落库，方案 §4.5）。
/// 真正落库由 DOC/DCT saver 钩子负责（§10.3，C.2.2 责任边界）。
pub async fn generate_batch(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(body): Json<BatchBody>,
) -> Json<ApiResp<Value>> {
    let db_id = db_id_from(&headers).await;
    run_batch(body, &db_id, "生成", false).await
}

#[derive(Deserialize)]
pub struct ValidateBody {
    pub code: String,
    #[serde(default)]
    pub pattern: Option<String>,
}

/// POST /api/code/validate —— manual pattern 校验。
pub async fn validate(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    _headers: HeaderMap,
    Json(body): Json<ValidateBody>,
) -> Json<ApiResp<Value>> {
    let pattern = match &body.pattern {
        Some(p) => p.clone(),
        None => return Json(ApiResp::ok(json!({ "valid": true, "reason": "无 pattern，跳过校验" }))),
    };

    let re = match regex::Regex::new(&pattern) {
        Ok(re) => re,
        Err(e) => {
            return Json(ApiResp::fail(
                400u16,
                &format!("正则编译失败：{e}"),
            ))
        }
    };

    let valid = re.is_match(&body.code);
    Json(ApiResp::ok(json!({
        "valid": valid,
        "code": body.code,
        "pattern": pattern,
    })))
}

// ═══════════════════════════════════════════════════════════════════════════════
// 断号查询 / 手动取号（C6）
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Deserialize)]
pub struct GapQuery {
    #[serde(default)]
    pub prefix: Option<String>,
}

/// GET /api/code/gaps —— 断号列表。
pub async fn gap_list(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    headers: HeaderMap,
    Query(q): Query<GapQuery>,
) -> Json<ApiResp<Value>> {
    let db_id = db_id_from(&headers).await;
    match gap_store::query_gaps(q.prefix.as_deref(), &db_id).await {
        Ok(gaps) => Json(ApiResp::ok(json!({ "gaps": gaps }))),
        Err(e) => err_resp(e),
    }
}

/// gap_take 请求体（强类型，替代 Json<Value> 手解析）。
#[derive(Deserialize)]
pub struct GapTakeBody {
    #[serde(default)]
    pub prefix: String,
    #[serde(default = "default_gap_width")]
    pub width: u64,
}

fn default_gap_width() -> u64 {
    4
}

/// POST /api/code/gaps/take —— 手动取一个断号填补。
pub async fn gap_take(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(body): Json<GapTakeBody>,
) -> Json<ApiResp<Value>> {
    let db_id = db_id_from(&headers).await;
    match gap_store::take_gap(&body.prefix, body.width as usize, &db_id).await {
        Ok(Some(serial)) => Json(ApiResp::ok(json!({
            "prefix": body.prefix,
            "serial": serial,
            "taken": true,
        }))),
        Ok(None) => Json(ApiResp::ok(json!({ "taken": false, "reason": "无断号" }))),
        Err(e) => err_resp(e),
    }
}
