/*
 * @Describe: cmx-flow 的薄 axum handler：提取参数 → 取引擎单例 → 调引擎/定义服务 → ApiResp 信封。
 *
 * 从 cmx-flow-demo/main.rs 移植。差异：
 *   - State(_s)/CmxSvrContext(_ctx) 提取器（RPT 风格，绑定不用，保留提取器顺序 + 未来可用）；
 *   - 经 crate::engine::flow() 取 OnceCell 单例，替代 demo 的 State<AppState>；
 *   - 返回 Result<Json<ApiResp<Value>>>，错误经 engine_err/def_err 桥到 cmx_api::Error。
 * 响应 JSON 形状与 demo 完全一致（前端依赖 key/name/state/activeVersion/bpmnXml/instances/tasks…）。
 */

use axum::Json;
use axum::extract::{Path, Query, State};
use serde::Deserialize;
use serde_json::{Value, json};

use cmx_api::middleware::CmxSvrContext;
use cmx_api::{ApiResp, CmxAppState, Result};

use cmx_flow_engine::{RuntimeStore, Variables};

use crate::engine::{FlowRuntime, IAM_DB_ID, flow};
use crate::views::{definition_view, instance_state_str, instance_view, summary_view};

// ————————————————————— 错误桥 —————————————————————

fn engine_err(e: cmx_flow_engine::Error) -> cmx_api::Error {
    cmx_biz::BizError::business(e.to_string()).into()
}
fn def_err(e: cmx_flow_def::DefError) -> cmx_api::Error {
    cmx_biz::BizError::business(e.to_string()).into()
}
fn msg_err(msg: String) -> cmx_api::Error {
    cmx_biz::BizError::business(msg).into()
}

/// 载入实例并返回视图信封（多个 handler 共用）。
async fn load_view(rt: &FlowRuntime, instance_id: &str) -> Result<Json<ApiResp<Value>>> {
    let snap = rt
        .engine
        .store()
        .load_snapshot(instance_id)
        .await
        .map_err(|e| msg_err(format!("载入实例失败: {e}")))?;
    Ok(Json(ApiResp::ok(instance_view(&snap))))
}

// ————————————————————— 定义（设计器） —————————————————————

/// 全部流程定义 → 前端画图用的 JSON（每个含节点 + 边）。来源引擎已装载定义（运行态视角）。
pub async fn get_definitions(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
) -> Result<Json<ApiResp<Value>>> {
    let rt = flow().await?;
    let defs: Vec<Value> = rt
        .definitions
        .read()
        .await
        .iter()
        .map(definition_view)
        .collect();
    Ok(Json(ApiResp::ok(json!({ "definitions": defs }))))
}

/// 设计器用的定义列表 → 来源定义库（草稿 + 已发布全都列，设计态视角）。
/// 与 get_definitions 区别：那个是引擎运行态已装载的；这个是库里所有可编辑的定义。
/// 每条附带版本序列（versions[]，版本号降序，含变更说明），activeVersion=当前生效版本。
pub async fn list_design_definitions(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
) -> Result<Json<ApiResp<Value>>> {
    let rt = flow().await?;
    let recs = rt.def_svc.list().await.map_err(def_err)?;
    // 一次取全部版本，按 def_key 分组（省 N+1）。
    let all_vers = rt.def_svc.list_all_versions().await.map_err(def_err)?;
    let defs: Vec<Value> = recs
        .iter()
        .map(|r| {
            let versions: Vec<Value> = all_vers
                .iter()
                .filter(|v| v.def_key == r.key)
                .map(version_meta_view)
                .collect();
            json!({
                "key": r.key,
                "name": r.name,
                "domain": r.domain,
                "application": r.application,
                "module": r.module,
                "state": r.state.as_str(),
                "activeVersion": r.active_version,
                "versionCount": versions.len(),
                "versions": versions,
                "startable": true,
            })
        })
        .collect();
    Ok(Json(ApiResp::ok(json!({ "definitions": defs }))))
}

/// 单条版本元信息 → 前端 JSON（对齐报表版本字段命名习惯）。
fn version_meta_view(v: &cmx_flow_def::VersionMeta) -> Value {
    json!({
        "version": v.version,
        "note": v.note,
        "publishedAt": v.published_at.to_rfc3339(),
        "publishedBy": v.published_by,
    })
}

/// 设计器：存草稿请求。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveDraftReq {
    name: String,
    #[serde(default)]
    domain: Option<String>,
    #[serde(default)]
    application: Option<String>,
    #[serde(default)]
    module: Option<String>,
    #[serde(default)]
    category: Option<String>,
    /// 设计器导出的 BPMN 2.0 XML。
    bpmn_xml: String,
    #[serde(default)]
    updated_by: Option<String>,
}

/// 设计器：存草稿（先试编译挡回非法 BPMN）。
pub async fn save_definition_draft(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Json(req): Json<SaveDraftReq>,
) -> Result<Json<ApiResp<Value>>> {
    let rt = flow().await?;
    let rec = rt
        .def_svc
        .save_draft(
            &req.name,
            req.domain,
            req.application,
            req.module,
            req.category,
            &req.bpmn_xml,
            req.updated_by,
        )
        .await
        .map_err(def_err)?;
    Ok(Json(ApiResp::ok(json!({
        "key": rec.key,
        "name": rec.name,
        "state": rec.state.as_str(),
        "activeVersion": rec.active_version,
    }))))
}

/// 设计器：取单个定义详情（含草稿 XML，供重新加载编辑）。
/// 可选 ?version=N：取指定历史版本的 XML（版本切换用），不传则取当前草稿。
#[derive(Deserialize)]
pub struct DetailQuery {
    #[serde(default)]
    version: Option<i32>,
}

pub async fn get_definition_detail(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Path(key): Path<String>,
    Query(q): Query<DetailQuery>,
) -> Result<Json<ApiResp<Value>>> {
    let rt = flow().await?;
    let rec = rt
        .def_svc
        .get(&key)
        .await
        .map_err(def_err)?
        .ok_or_else(|| msg_err(format!("定义不存在: {key}")))?;
    // 版本列表（降序）。
    let versions: Vec<Value> = rt
        .def_svc
        .list_versions(&key)
        .await
        .map_err(def_err)?
        .iter()
        .map(version_meta_view)
        .collect();
    // ?version=N → 取该历史版本 XML；否则用当前草稿。
    let (xml, shown_version) = if let Some(vn) = q.version {
        let ver = rt
            .def_svc
            .get_version(&key, vn)
            .await
            .map_err(def_err)?
            .ok_or_else(|| msg_err(format!("版本不存在: {key}@v{vn}")))?;
        (Some(ver.bpmn_xml), Some(vn))
    } else {
        (rec.draft_xml.clone(), rec.active_version)
    };
    Ok(Json(ApiResp::ok(json!({
        "key": rec.key,
        "name": rec.name,
        "domain": rec.domain,
        "application": rec.application,
        "module": rec.module,
        "category": rec.category,
        "state": rec.state.as_str(),
        "activeVersion": rec.active_version,
        "shownVersion": shown_version,
        "versions": versions,
        "bpmnXml": xml,
        "updatedAt": rec.updated_at.to_rfc3339(),
    }))))
}

/// 设计器：发布请求。note = 本次发布的变更说明（可空）。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishReq {
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    published_by: Option<String>,
}

/// 设计器：发布（草稿 → 版本 +1）。新版下次服务重启装载生效。
pub async fn publish_definition(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Path(key): Path<String>,
    Json(req): Json<PublishReq>,
) -> Result<Json<ApiResp<Value>>> {
    let rt = flow().await?;
    let version = rt
        .def_svc
        .publish(&key, req.note, req.published_by)
        .await
        .map_err(def_err)?;
    Ok(Json(ApiResp::ok(json!({
        "key": key,
        "version": version,
        "note": "已发布；重启服务后引擎装载新版（热更列入后续阶段）",
    }))))
}

/// 设计器：列某定义的全部版本（版本号降序）。
pub async fn list_definition_versions(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Path(key): Path<String>,
) -> Result<Json<ApiResp<Value>>> {
    let rt = flow().await?;
    let rec = rt
        .def_svc
        .get(&key)
        .await
        .map_err(def_err)?
        .ok_or_else(|| msg_err(format!("定义不存在: {key}")))?;
    let versions: Vec<Value> = rt
        .def_svc
        .list_versions(&key)
        .await
        .map_err(def_err)?
        .iter()
        .map(version_meta_view)
        .collect();
    Ok(Json(ApiResp::ok(json!({
        "key": rec.key,
        "activeVersion": rec.active_version,
        "versions": versions,
    }))))
}

/// 设计器：激活指定版本为当前生效版本（对标报表「设为默认版本」）。重启后引擎装载生效。
pub async fn activate_definition_version(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Path((key, version)): Path<(String, i32)>,
) -> Result<Json<ApiResp<Value>>> {
    let rt = flow().await?;
    rt.def_svc
        .activate_version(&key, version)
        .await
        .map_err(def_err)?;
    Ok(Json(ApiResp::ok(json!({
        "key": key,
        "activeVersion": version,
        "note": "已设为当前版本；重启服务后引擎装载生效",
    }))))
}

/// 设计器：删除某历史版本（不能删当前生效版本）。
pub async fn delete_definition_version(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Path((key, version)): Path<(String, i32)>,
) -> Result<Json<ApiResp<Value>>> {
    let rt = flow().await?;
    rt.def_svc
        .delete_version(&key, version)
        .await
        .map_err(def_err)?;
    Ok(Json(ApiResp::ok(
        json!({ "key": key, "deletedVersion": version }),
    )))
}

// ————————————————————— 实例 —————————————————————

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartReq {
    #[serde(default)]
    definition_key: Option<String>,
    applicant: String,
    amount: f64,
    #[serde(default)]
    approvers: Option<Vec<String>>,
    #[serde(default)]
    org_id: Option<String>,
}

/// 启动一个流程实例。
pub async fn start_instance(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Json(req): Json<StartReq>,
) -> Result<Json<ApiResp<Value>>> {
    let rt = flow().await?;
    let def_key = req
        .definition_key
        .clone()
        .unwrap_or_else(|| "credit_approval".to_string());

    let mut vars = Variables::new();
    vars.set("applicant", json!(req.applicant));
    vars.set("amount", json!(req.amount));
    if let Some(approvers) = &req.approvers {
        vars.set("approvers", json!(approvers));
    }

    let biz_key = format!("CR-{}", req.applicant);
    let result = rt
        .engine
        .start_process_org(&def_key, vars, Some(biz_key), req.org_id.clone())
        .await
        .map_err(engine_err)?;

    load_view(rt, &result.instance_id).await
}

/// 列全部实例。
pub async fn list_instances(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
) -> Result<Json<ApiResp<Value>>> {
    let rt = flow().await?;
    let summaries = rt
        .engine
        .store()
        .list_instances(100)
        .await
        .map_err(|e| msg_err(format!("查询实例列表失败: {e}")))?;
    let instances: Vec<Value> = summaries.iter().map(summary_view).collect();
    Ok(Json(ApiResp::ok(json!({ "instances": instances }))))
}

/// 单实例详情。
pub async fn get_instance(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Path(id): Path<String>,
) -> Result<Json<ApiResp<Value>>> {
    let rt = flow().await?;
    let snap = rt
        .engine
        .store()
        .load_snapshot(&id)
        .await
        .map_err(|_| msg_err(format!("实例不存在: {id}")))?;
    Ok(Json(ApiResp::ok(instance_view(&snap))))
}

/// 列某实例的子实例（M5.1 子流程）。
pub async fn get_children(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Path(id): Path<String>,
) -> Result<Json<ApiResp<Value>>> {
    let rt = flow().await?;
    let children = rt
        .engine
        .store()
        .find_child_instances(&id)
        .await
        .map_err(|e| msg_err(format!("查询子实例失败: {e}")))?;
    let mut items = Vec::new();
    for c in &children {
        if let Ok(snap) = rt.engine.store().load_snapshot(&c.id).await {
            items.push(instance_view(&snap));
        }
    }
    Ok(Json(ApiResp::ok(json!({ "children": items }))))
}

#[derive(Deserialize)]
pub struct CancelReq {
    #[serde(default)]
    reason: Option<String>,
}

/// 撤单 / 取消一个流程实例（M3）。
pub async fn cancel_instance(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Path(id): Path<String>,
    Json(req): Json<CancelReq>,
) -> Result<Json<ApiResp<Value>>> {
    let rt = flow().await?;
    rt.engine
        .cancel_process(&id, req.reason)
        .await
        .map_err(engine_err)?;
    load_view(rt, &id).await
}

// ————————————————————— 任务 —————————————————————

#[derive(Deserialize)]
pub struct CompleteReq {
    instance_id: String,
    #[serde(default)]
    decision: Option<String>,
}

/// 办结一个任务。
pub async fn complete_task(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Path(task_id): Path<String>,
    Json(req): Json<CompleteReq>,
) -> Result<Json<ApiResp<Value>>> {
    let rt = flow().await?;
    let mut vars = Variables::new();
    if let Some(d) = &req.decision {
        vars.set("lastDecision", json!(d));
    }
    rt.engine
        .complete_task(&req.instance_id, &task_id, vars)
        .await
        .map_err(engine_err)?;
    load_view(rt, &req.instance_id).await
}

#[derive(Deserialize)]
pub struct ClaimReq {
    instance_id: String,
    user_id: String,
}

/// 认领一个候选任务（M4.1）。
pub async fn claim_task(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Path(task_id): Path<String>,
    Json(req): Json<ClaimReq>,
) -> Result<Json<ApiResp<Value>>> {
    let rt = flow().await?;
    rt.engine
        .claim_task(&req.instance_id, &task_id, &req.user_id)
        .await
        .map_err(engine_err)?;
    load_view(rt, &req.instance_id).await
}

#[derive(Deserialize)]
pub struct TransferReq {
    instance_id: String,
    from_user: String,
    to_user: String,
    #[serde(default)]
    reason: Option<String>,
}

/// 转办（M4.3）。
pub async fn transfer_task(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Path(task_id): Path<String>,
    Json(req): Json<TransferReq>,
) -> Result<Json<ApiResp<Value>>> {
    let rt = flow().await?;
    rt.engine
        .transfer_task(
            &req.instance_id,
            &task_id,
            &req.from_user,
            &req.to_user,
            req.reason.as_deref(),
        )
        .await
        .map_err(engine_err)?;
    load_view(rt, &req.instance_id).await
}

/// 委派（M4.3）。
pub async fn delegate_task(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Path(task_id): Path<String>,
    Json(req): Json<TransferReq>,
) -> Result<Json<ApiResp<Value>>> {
    let rt = flow().await?;
    rt.engine
        .delegate_task(
            &req.instance_id,
            &task_id,
            &req.from_user,
            &req.to_user,
            req.reason.as_deref(),
        )
        .await
        .map_err(engine_err)?;
    load_view(rt, &req.instance_id).await
}

#[derive(Deserialize)]
pub struct AddSignReq {
    instance_id: String,
    from_user: String,
    to_user: String,
    #[serde(default = "default_true")]
    before: bool,
    #[serde(default)]
    reason: Option<String>,
}
fn default_true() -> bool {
    true
}

/// 加签（M4.3）。
pub async fn add_sign_task(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Path(task_id): Path<String>,
    Json(req): Json<AddSignReq>,
) -> Result<Json<ApiResp<Value>>> {
    let rt = flow().await?;
    rt.engine
        .add_sign(
            &req.instance_id,
            &task_id,
            &req.from_user,
            &req.to_user,
            req.before,
            req.reason.as_deref(),
        )
        .await
        .map_err(engine_err)?;
    load_view(rt, &req.instance_id).await
}

// ————————————————————— 抄送 / 定时器 / 用户 —————————————————————

/// 手动「立即检查到期定时器」（M2.5）。
pub async fn trigger_timers(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
) -> Result<Json<ApiResp<Value>>> {
    let rt = flow().await?;
    let fired = rt
        .engine
        .trigger_due_timers(100)
        .await
        .map_err(engine_err)?;
    let items: Vec<Value> = fired
        .iter()
        .map(|f| {
            json!({
                "instanceId": f.instance_id,
                "boundaryBpmnId": f.boundary_bpmn_id,
                "cancelActivity": f.cancel_activity,
                "instanceState": instance_state_str(f.instance_state),
            })
        })
        .collect();
    Ok(Json(ApiResp::ok(
        json!({ "firedCount": fired.len(), "fired": items }),
    )))
}

/// 列出 IAM 库用户（id → 昵称/用户名），供前端把候选人 id 显示成友好名字。
pub async fn list_users(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
) -> Result<Json<ApiResp<Value>>> {
    let ds = cmx_database_pg::query_sql(
        IAM_DB_ID,
        None,
        "SELECT id, username, nickname FROM cmx_user WHERE archived = 0 ORDER BY create_time LIMIT 200",
        "flow_list_users",
    )
    .await
    .map_err(|e| msg_err(format!("查询用户失败: {e}")))?;
    let schema = ds.schema.as_ref();
    let get = |row: &cmx_core::model::data::dataset::Row, col: &str| -> Option<String> {
        match row.get_by_name(schema, col) {
            Some(cmx_core::model::cell::DataValue::String(s)) => Some(s.clone()),
            _ => None,
        }
    };
    let users: Vec<Value> = ds
        .iter()
        .map(|row| {
            let id = get(row, "id").unwrap_or_default();
            let name = get(row, "nickname")
                .filter(|s| !s.is_empty())
                .or_else(|| get(row, "username"))
                .unwrap_or_else(|| id.clone());
            json!({ "id": id, "name": name })
        })
        .collect();
    Ok(Json(ApiResp::ok(json!({ "users": users }))))
}

#[derive(Deserialize)]
pub struct CcQuery {
    user: String,
    #[serde(default)]
    unread: bool,
}

/// 「抄送我的」列表（M4.2）。
pub async fn list_cc(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Query(q): Query<CcQuery>,
) -> Result<Json<ApiResp<Value>>> {
    let rt = flow().await?;
    let items = rt
        .engine
        .cc_for_user(&q.user, q.unread, 100)
        .await
        .map_err(engine_err)?;
    let cc: Vec<Value> = items
        .iter()
        .map(|c| {
            json!({
                "id": c.id,
                "instanceId": c.instance_id,
                "businessKey": c.business_key,
                "definitionKey": c.definition_key,
                "nodeBpmnId": c.node_bpmn_id,
                "reason": c.reason,
                "read": c.read,
                "createdAt": c.created_at.to_rfc3339(),
            })
        })
        .collect();
    Ok(Json(ApiResp::ok(json!({ "cc": cc }))))
}

/// 标记一条抄送已读（M4.2）。
pub async fn mark_cc_read(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Path(cc_id): Path<String>,
) -> Result<Json<ApiResp<Value>>> {
    let rt = flow().await?;
    let ok = rt.engine.mark_cc_read(&cc_id).await.map_err(engine_err)?;
    Ok(Json(ApiResp::ok(json!({ "ok": ok }))))
}

// ————————————————————— 子流程组织路由（绑定管理） —————————————————————

/// 组织树（设计器「按组织配置子流程」的组织选择器）。扁平表 + path，前端建树。
pub async fn list_orgs(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
) -> Result<Json<ApiResp<Value>>> {
    let rt = flow().await?;
    let orgs = rt.binding_store.list_orgs().await.map_err(msg_err)?;
    let items: Vec<Value> = orgs
        .iter()
        .map(|o| {
            json!({
                "id": o.id,
                "name": o.name,
                "parentId": o.parent_id,
                "path": o.path,
            })
        })
        .collect();
    Ok(Json(ApiResp::ok(json!({ "orgs": items }))))
}

/// 列某逻辑子流程 key 的全部组织绑定（含默认兜底），带组织名。
pub async fn list_subflow_bindings(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Path(called_key): Path<String>,
) -> Result<Json<ApiResp<Value>>> {
    let rt = flow().await?;
    let list = rt
        .binding_store
        .list_by_key(&called_key)
        .await
        .map_err(msg_err)?;
    let items: Vec<Value> = list.iter().map(binding_view).collect();
    Ok(Json(ApiResp::ok(
        json!({ "calledKey": called_key, "bindings": items }),
    )))
}

fn binding_view(b: &cmx_flow_store_pg::SubflowBinding) -> Value {
    json!({
        "id": b.id,
        "calledKey": b.called_key,
        "orgId": b.org_id,
        "orgName": b.org_name,
        "targetKey": b.target_definition_key,
        "enabled": b.enabled,
        "remark": b.remark,
        "isDefault": b.org_id.is_none(),
    })
}

/// upsert 绑定请求。orgId 为空/缺省 = 默认兜底绑定。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertBindingReq {
    /// 逻辑子流程 key（= callActivity cmx:calledKey）。
    called_key: String,
    /// 组织 id（None/空 → 默认兜底绑定）。
    #[serde(default)]
    org_id: Option<String>,
    /// 目标子流程定义 key。
    target_key: String,
    #[serde(default = "default_true")]
    enabled: bool,
    #[serde(default)]
    remark: Option<String>,
}

/// upsert 一条组织绑定（同 called_key+org 视为一条）。id 由 called_key+org 派生（幂等）。
pub async fn upsert_subflow_binding(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Json(req): Json<UpsertBindingReq>,
) -> Result<Json<ApiResp<Value>>> {
    let rt = flow().await?;
    let org = req.org_id.as_deref().filter(|s| !s.is_empty());
    // 派生稳定 id：便于同 (key,org) 反复保存不产生多行（upsert 内也会先删同键旧行）。
    let id = binding_id(&req.called_key, org);
    rt.binding_store
        .upsert(
            &id,
            &req.called_key,
            org,
            &req.target_key,
            req.enabled,
            req.remark.as_deref(),
        )
        .await
        .map_err(msg_err)?;
    Ok(Json(ApiResp::ok(json!({ "id": id }))))
}

/// 从 called_key + org 派生稳定绑定 id（非加密，仅去重定位用）。
fn binding_id(called_key: &str, org: Option<&str>) -> String {
    let raw = format!("{called_key}|{}", org.unwrap_or("__default__"));
    // 简单 FNV-1a，避免引 uuid/sha 依赖；碰撞面为同库同 key，可忽略。
    let mut h: u64 = 0xcbf29ce484222325;
    for byte in raw.as_bytes() {
        h ^= *byte as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("sb_{h:016x}")
}

/// 删除一条绑定（按 id）。
pub async fn delete_subflow_binding(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    Path(id): Path<String>,
) -> Result<Json<ApiResp<Value>>> {
    let rt = flow().await?;
    rt.binding_store.delete(&id).await.map_err(msg_err)?;
    Ok(Json(ApiResp::ok(json!({ "deleted": id }))))
}
