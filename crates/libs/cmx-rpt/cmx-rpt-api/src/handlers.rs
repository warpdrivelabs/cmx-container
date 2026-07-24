//! 报表模块的薄 axum handler：提取参数 → 调 cmx_rpt_store_pg 服务 → ApiResp 信封。
//!
//! 签名保留 `State<CmxAppState>`/`CmxSvrContext` 提取器（虽当前不使用，绑为 `_s`/`_ctx`，
//! 与迁移前完全一致，保留提取器顺序 + 未来可用）。save_layout 保留 409 裸响应语义。

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use serde::Deserialize;
use serde_json::{Value, json};

use cmx_api::CmxAppState;
use cmx_api::middleware::CmxSvrContext;
use cmx_api::{ApiResp, Result};

use cmx_rpt_model::{CreateVersionBody, LayoutQuery, ReportDetailQuery, ReportListQuery};
use cmx_rpt_store_pg as store;

pub async fn report_design_overview(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
) -> Result<Json<ApiResp<Value>>> {
    Ok(Json(ApiResp::ok(store::overview().await?)))
}

pub async fn report_design_reports(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Query(q): Query<ReportListQuery>,
) -> Result<Json<ApiResp<Value>>> {
    Ok(Json(ApiResp::ok(store::reports(&q).await?)))
}

pub async fn report_design_elements(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
) -> Result<Json<ApiResp<Value>>> {
    Ok(Json(ApiResp::ok(store::elements().await?)))
}

pub async fn report_design_calendar(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
) -> Result<Json<ApiResp<Value>>> {
    Ok(Json(ApiResp::ok(store::calendar().await?)))
}

pub async fn report_design_consol_org(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
) -> Result<Json<ApiResp<Value>>> {
    Ok(Json(ApiResp::ok(store::consol_org().await?)))
}

pub async fn report_design_report_detail(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Path(code): Path<String>,
    Query(q): Query<ReportDetailQuery>,
) -> Result<Json<ApiResp<Value>>> {
    Ok(Json(ApiResp::ok(
        store::report_detail(&code, q.version).await?,
    )))
}

pub async fn report_design_create_report(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    body: Json<Value>,
) -> Result<Json<ApiResp<Value>>> {
    Ok(Json(ApiResp::ok(store::create_report(&body.0).await?)))
}

pub async fn report_design_delete_report(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Path(code): Path<String>,
) -> Result<Json<ApiResp<Value>>> {
    Ok(Json(ApiResp::ok(store::delete_report(&code).await?)))
}

pub async fn report_design_create_version(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Path(report_code): Path<String>,
    body: Option<Json<CreateVersionBody>>,
) -> Result<Json<ApiResp<Value>>> {
    let body = body.map(|b| b.0).unwrap_or_default();
    Ok(Json(ApiResp::ok(
        store::create_version(&report_code, &body).await?,
    )))
}

pub async fn report_design_set_default_version(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Path((report_code, version)): Path<(String, String)>,
) -> Result<Json<ApiResp<Value>>> {
    Ok(Json(ApiResp::ok(
        store::set_default_version(&report_code, &version).await?,
    )))
}

pub async fn report_design_load_layout(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Path(code): Path<String>,
    Query(q): Query<LayoutQuery>,
) -> Result<Json<ApiResp<Value>>> {
    Ok(Json(ApiResp::ok(store::load_layout(&code, &q).await?)))
}

pub async fn report_design_save_layout(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Path(code): Path<String>,
    body: Json<Value>,
) -> Result<axum::response::Response> {
    use axum::response::IntoResponse;
    match store::save_layout(&code, &body.0).await? {
        // 乐观锁冲突：保留原裸 409 响应体（不走 ApiResp 信封）。
        store::SaveLayoutOutcome::Conflict => Ok((
            axum::http::StatusCode::CONFLICT,
            Json(json!({ "code": 409, "msg": "版式已被他人更新，请刷新后重试" })),
        )
            .into_response()),
        store::SaveLayoutOutcome::Ok(payload) => Ok(Json(ApiResp::ok(payload)).into_response()),
    }
}

pub async fn report_design_query_data(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Path(code): Path<String>,
    body: Json<Value>,
) -> Result<Json<ApiResp<Value>>> {
    Ok(Json(ApiResp::ok(store::query_data(&code, &body.0).await?)))
}

/// 打开报表：一次调用取全集（版式+cellMap+元素+函数[+数据]），替代前端顺序多调。
/// body: { version?, orgCode?, periodCode? }；org+period 齐备则并入数据（应用器），否则 cells=[]（设计器）。
pub async fn report_design_open_report(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Path(code): Path<String>,
    body: Option<Json<Value>>,
) -> Result<Json<ApiResp<Value>>> {
    let body = body.map(|b| b.0).unwrap_or_else(|| json!({}));
    Ok(Json(ApiResp::ok(store::open_report(&code, &body).await?)))
}

pub async fn report_design_save_data(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Path(code): Path<String>,
    body: Json<Value>,
) -> Result<Json<ApiResp<Value>>> {
    Ok(Json(ApiResp::ok(store::save_data(&code, &body.0).await?)))
}

// ============================================================================
// 计算态：报表函数计算（真算，替换旧 stub）+ 函数目录（供设计器向导）。
// ============================================================================

/// 计算报表：装载公式 → 递归求值（两遍法取数 + REF 跨表 + 循环检测）→ 落 cr_cell_data。
/// body: { version?, orgCode, periodCode }。
pub async fn report_design_compute(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Path(code): Path<String>,
    body: Json<Value>,
) -> Result<Json<ApiResp<Value>>> {
    Ok(Json(ApiResp::ok(
        store::compute_report_service(&code, &body.0).await?,
    )))
}

/// 函数目录：枚举已注册报表函数元数据（名/分类/原型/向导/帮助/示例），供设计器向导渲染。
pub async fn report_design_functions(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
) -> Result<Json<ApiResp<Value>>> {
    Ok(Json(ApiResp::ok(cmx_rpt_formula::catalog_json())))
}

// ============================================================================
// 协同编辑 B 档：操作日志（提交语义操作 + 增量拉取追平）。
// ============================================================================

/// 提交一批语义操作：咨询锁串行化 → 幂等/冲突判定 → 投影增量 UPSERT → append op_log。
/// body: { version, ops: [{type,target,payload,baseSeq,clientOpId}] }。actor 取 AuthContext。
pub async fn report_design_apply_ops(
    State(_s): State<CmxAppState>,
    CmxSvrContext(ctx): CmxSvrContext,
    Path(code): Path<String>,
    body: Json<Value>,
) -> Result<Json<ApiResp<Value>>> {
    let (actor_id, actor_name) = match ctx.auth_context.as_ref() {
        Some(a) => (a.user_id.clone(), a.username.clone()),
        None => (String::new(), String::new()),
    };
    Ok(Json(ApiResp::ok(
        store::apply_ops(&code, &body.0, &actor_id, &actor_name).await?,
    )))
}

/// 拉取操作日志增量：seq > since（打开重放 + 编辑中追平轮询）。
#[derive(Debug, Deserialize)]
pub struct OpsQuery {
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub since: Option<i64>,
    #[serde(default)]
    pub limit: Option<i64>,
}

pub async fn report_design_list_ops(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Path(code): Path<String>,
    Query(q): Query<OpsQuery>,
) -> Result<Json<ApiResp<Value>>> {
    let version = q.version.unwrap_or_default(); // ''=默认版本，与 layout 端点一致
    Ok(Json(ApiResp::ok(
        store::list_ops(&code, &version, q.since.unwrap_or(0), q.limit.unwrap_or(500)).await?,
    )))
}

// ============================================================================
// 旧报表设计器兼容计算接口（stub，无 DB）：恢复历史 html 预览入口避免 404。
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct RptComputeReq {
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub application: Option<String>,
    #[serde(default)]
    pub module: Option<String>,
    #[serde(default)]
    pub file: Option<String>,
    #[serde(default)]
    pub org: Option<String>,
    #[serde(default)]
    pub period: Option<String>,
}

pub async fn rpt_compute(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    _headers: HeaderMap,
    Json(req): Json<RptComputeReq>,
) -> Result<Json<ApiResp<Value>>> {
    let def_ref = json!({
        "domain": req.domain,
        "application": req.application,
        "module": req.module,
        "file": req.file,
    });
    Ok(Json(ApiResp::ok(json!({
        "values": {},
        "fetches": {},
        "context": {
            "org": req.org,
            "period": req.period,
        },
        "definition": def_ref,
        "message": "旧报表设计器兼容预览接口已恢复；新报表设计工作台使用 /api/report-design/*。",
    }))))
}

// ============================================================================
// 计算路由设置：报表取数路由绑定（cr_report_source_binding）的注册 CRUD
//   单据类型(sourceKey) + 组织(orgId，空=默认兜底) → 物理目标(targetKind + targetRef)
//   仅注册；运行时三层继承解析与 scatter-gather 取数后续接入。
// ============================================================================

/// 列某单据类型逻辑名的全部路由绑定（含默认兜底）。
pub async fn list_report_source_bindings(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Path(key): Path<String>,
) -> Result<Json<ApiResp<Value>>> {
    Ok(Json(ApiResp::ok(store::list_source_bindings(&key).await?)))
}

/// upsert 请求体。orgId 为空/缺省 = 默认兜底绑定；同 (sourceKey, orgId) 视为一条。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertSourceBindingReq {
    source_key: String,
    #[serde(default)]
    org_id: Option<String>,
    target_kind: String,
    target_ref: String,
    #[serde(default)]
    transport: Option<String>,
    #[serde(default)]
    priority: i64,
    #[serde(default = "default_true")]
    enabled: bool,
    #[serde(default)]
    remark: Option<String>,
}

fn default_true() -> bool {
    true
}

/// upsert 一条路由绑定。id 由 sourceKey + org 派生（幂等）。
pub async fn upsert_report_source_binding(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Json(req): Json<UpsertSourceBindingReq>,
) -> Result<Json<ApiResp<Value>>> {
    let org = req.org_id.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let id = store::binding_id(&req.source_key, org);
    let out = store::upsert_source_binding(
        &id,
        &req.source_key,
        org,
        &req.target_kind,
        &req.target_ref,
        req.transport.as_deref(),
        req.priority,
        req.enabled,
        req.remark.as_deref(),
    )
    .await?;
    Ok(Json(ApiResp::ok(out)))
}

/// 删除一条路由绑定（按 id）。
pub async fn delete_report_source_binding(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Path(id): Path<String>,
) -> Result<Json<ApiResp<Value>>> {
    Ok(Json(ApiResp::ok(store::delete_source_binding(&id).await?)))
}
