//! 互斥规则管理 handler
//!
//! 提供规则 CRUD、启用/禁用、规则项管理、校验测试等 API。

use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use tracing::debug;

use cmx_iam::rule::entity::{
    CreateExclusionRuleRequest, ExclusionRule, ExclusionRuleItem, UpdateExclusionRuleRequest,
    ValidateRuleRequest, ValidateRuleResponse,
};

use crate::app_state::CmxAppState;
use crate::middleware::CmxSvrContext;
use crate::{ApiResp, Error, Result};

/// 启用/禁用互斥规则请求载荷。
///
/// 通过 `status` 字段切换规则的启用状态，禁用后规则不再参与互斥校验。
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct ToggleRuleStatusRequest {
    /// 规则 ID。
    pub rule_id: String,
    /// 目标状态：0-禁用，1-启用。
    pub status: i64,
}

/// 添加互斥规则项请求载荷。
///
/// 在指定规则下批量追加互斥对象（权限或角色，取决于规则的 subject_type）。
/// 主对象不可重复出现在互斥对象列表中。
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct AddRuleItemsRequest {
    /// 目标规则 ID。
    pub rule_id: String,
    /// 待添加的互斥对象 ID 列表。
    pub subject_ids: Vec<String>,
}

/// 移除互斥规则项请求载荷。
///
/// 按 item_id 批量删除规则下的互斥对象项。删除后用户集不再受此规则中对应项约束。
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct RemoveRuleItemsRequest {
    /// 目标规则 ID。
    pub rule_id: String,
    /// 待移除的规则项 ID 列表（cmx_exclusion_rule_item.id）。
    pub item_ids: Vec<String>,
}

/// 规则详情响应载荷（含规则项）。
///
/// 用于规则详情页：返回规则主体与该规则下所有互斥对象项。
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct RuleDetailResponse {
    /// 规则主体。
    pub rule: ExclusionRule,
    /// 规则下的互斥对象项列表。
    pub items: Vec<ExclusionRuleItem>,
}

/// 批量操作通用响应载荷。
///
/// 用于 add_rule_items / remove_rule_items 等批量操作，返回实际写入或删除的记录数。
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct BatchResponse {
    /// 受影响的记录数。
    pub affected: u64,
}

/// 分页查询互斥规则请求载荷。
///
/// 采用页码 + 页大小模式，页码从 1 开始。
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct PageRulesRequest {
    /// 当前页码，从 1 开始。
    pub current: u64,
    /// 每页大小。
    pub size: u64,
}

/// 分页查询互斥规则响应载荷。
///
/// 包含当前页规则列表与总记录数。
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct PageRulesResponse {
    /// 规则列表。
    pub rules: Vec<ExclusionRule>,
    /// 总记录数。
    pub total: i64,
}

/// 创建规则
#[utoipa::path(
    post,
    path = "/api/iam/exclusion-rules/create",
    request_body = CreateExclusionRuleRequest,
    responses(
        (status = 200, description = "创建成功", body = ApiResp<ExclusionRule>)
    ),
    tag = "IAM-Exclusion"
)]
pub async fn create_rule(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(svr_ctx): CmxSvrContext,
    Json(req): Json<CreateExclusionRuleRequest>,
) -> Result<Json<ApiResp<ExclusionRule>>> {
    debug!(
        "{:<12} - handler::create_rule - code: {}",
        "HANDLER", req.code
    );

    let iam = cmx_state
        .iam()
        .ok_or_else(|| Error::business_error("IAM 服务未初始化".to_string()))?;

    let rule_service = iam
        .rule_service
        .as_ref()
        .ok_or_else(|| Error::business_error("互斥规则服务未初始化".to_string()))?;

    let rule = rule_service
        .create_rule(&svr_ctx, req)
        .await
        .map_err(|e| Error::business_error(e.to_string()))?;

    Ok(Json(ApiResp::ok(rule)))
}

/// 更新规则
#[utoipa::path(
    post,
    path = "/api/iam/exclusion-rules/update/{rule_id}",
    request_body = UpdateExclusionRuleRequest,
    params(
        ("rule_id" = String, Path, description = "规则ID")
    ),
    responses(
        (status = 200, description = "更新成功", body = ApiResp<ExclusionRule>)
    ),
    tag = "IAM-Exclusion"
)]
pub async fn update_rule(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(svr_ctx): CmxSvrContext,
    Path(rule_id): Path<String>,
    Json(data): Json<UpdateExclusionRuleRequest>,
) -> Result<Json<ApiResp<ExclusionRule>>> {
    debug!(
        "{:<12} - handler::update_rule - rule_id: {}",
        "HANDLER", rule_id
    );

    let iam = cmx_state
        .iam()
        .ok_or_else(|| Error::business_error("IAM 服务未初始化".to_string()))?;

    let rule_service = iam
        .rule_service
        .as_ref()
        .ok_or_else(|| Error::business_error("互斥规则服务未初始化".to_string()))?;

    let rule = rule_service
        .update_rule(&svr_ctx, &rule_id, data)
        .await
        .map_err(|e| Error::business_error(e.to_string()))?;

    Ok(Json(ApiResp::ok(rule)))
}

/// 删除规则
#[utoipa::path(
    post,
    path = "/api/iam/exclusion-rules/delete/{rule_id}",
    params(
        ("rule_id" = String, Path, description = "规则ID")
    ),
    responses(
        (status = 200, description = "删除成功", body = ApiResp<serde_json::Value>)
    ),
    tag = "IAM-Exclusion"
)]
pub async fn delete_rule(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(svr_ctx): CmxSvrContext,
    Path(rule_id): Path<String>,
) -> Result<Json<ApiResp<()>>> {
    debug!(
        "{:<12} - handler::delete_rule - rule_id: {}",
        "HANDLER", rule_id
    );

    let iam = cmx_state
        .iam()
        .ok_or_else(|| Error::business_error("IAM 服务未初始化".to_string()))?;

    let rule_service = iam
        .rule_service
        .as_ref()
        .ok_or_else(|| Error::business_error("互斥规则服务未初始化".to_string()))?;

    rule_service
        .delete_rule(&svr_ctx, &rule_id)
        .await
        .map_err(|e| Error::business_error(e.to_string()))?;

    Ok(Json(ApiResp::ok(())))
}

/// 查询规则详情
#[utoipa::path(
    get,
    path = "/api/iam/exclusion-rules/get/{rule_id}",
    params(
        ("rule_id" = String, Path, description = "规则ID")
    ),
    responses(
        (status = 200, description = "查询成功", body = ApiResp<RuleDetailResponse>)
    ),
    tag = "IAM-Exclusion"
)]
pub async fn get_rule(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Path(rule_id): Path<String>,
) -> Result<Json<ApiResp<RuleDetailResponse>>> {
    debug!(
        "{:<12} - handler::get_rule - rule_id: {}",
        "HANDLER", rule_id
    );

    let iam = cmx_state
        .iam()
        .ok_or_else(|| Error::business_error("IAM 服务未初始化".to_string()))?;

    let rule_service = iam
        .rule_service
        .as_ref()
        .ok_or_else(|| Error::business_error("互斥规则服务未初始化".to_string()))?;

    let (rule, items) = rule_service
        .get_rule(&rule_id)
        .await
        .map_err(|e| Error::business_error(e.to_string()))?;

    Ok(Json(ApiResp::ok(RuleDetailResponse { rule, items })))
}

/// 启用/禁用规则
#[utoipa::path(
    post,
    path = "/api/iam/exclusion-rules/toggle-status",
    request_body = ToggleRuleStatusRequest,
    responses(
        (status = 200, description = "切换成功", body = ApiResp<serde_json::Value>)
    ),
    tag = "IAM-Exclusion"
)]
pub async fn toggle_rule_status(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(svr_ctx): CmxSvrContext,
    Json(req): Json<ToggleRuleStatusRequest>,
) -> Result<Json<ApiResp<()>>> {
    debug!(
        "{:<12} - handler::toggle_rule_status - rule_id: {}, status: {}",
        "HANDLER", req.rule_id, req.status
    );

    let iam = cmx_state
        .iam()
        .ok_or_else(|| Error::business_error("IAM 服务未初始化".to_string()))?;

    let rule_service = iam
        .rule_service
        .as_ref()
        .ok_or_else(|| Error::business_error("互斥规则服务未初始化".to_string()))?;

    rule_service
        .toggle_rule_status(&svr_ctx, &req.rule_id, req.status)
        .await
        .map_err(|e| Error::business_error(e.to_string()))?;

    Ok(Json(ApiResp::ok(())))
}

/// 添加规则项
#[utoipa::path(
    post,
    path = "/api/iam/exclusion-rules/items/add",
    request_body = AddRuleItemsRequest,
    responses(
        (status = 200, description = "添加成功", body = ApiResp<BatchResponse>)
    ),
    tag = "IAM-Exclusion"
)]
pub async fn add_rule_items(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(svr_ctx): CmxSvrContext,
    Json(req): Json<AddRuleItemsRequest>,
) -> Result<Json<ApiResp<BatchResponse>>> {
    debug!(
        "{:<12} - handler::add_rule_items - rule_id: {}, count: {}",
        "HANDLER", req.rule_id, req.subject_ids.len()
    );

    let iam = cmx_state
        .iam()
        .ok_or_else(|| Error::business_error("IAM 服务未初始化".to_string()))?;

    let rule_service = iam
        .rule_service
        .as_ref()
        .ok_or_else(|| Error::business_error("互斥规则服务未初始化".to_string()))?;

    let affected = rule_service
        .add_rule_items(&svr_ctx, &req.rule_id, req.subject_ids)
        .await
        .map_err(|e| Error::business_error(e.to_string()))?;

    Ok(Json(ApiResp::ok(BatchResponse { affected })))
}

/// 移除规则项
#[utoipa::path(
    post,
    path = "/api/iam/exclusion-rules/items/remove",
    request_body = RemoveRuleItemsRequest,
    responses(
        (status = 200, description = "移除成功", body = ApiResp<BatchResponse>)
    ),
    tag = "IAM-Exclusion"
)]
pub async fn remove_rule_items(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(svr_ctx): CmxSvrContext,
    Json(req): Json<RemoveRuleItemsRequest>,
) -> Result<Json<ApiResp<BatchResponse>>> {
    debug!(
        "{:<12} - handler::remove_rule_items - rule_id: {}, count: {}",
        "HANDLER", req.rule_id, req.item_ids.len()
    );

    let iam = cmx_state
        .iam()
        .ok_or_else(|| Error::business_error("IAM 服务未初始化".to_string()))?;

    let rule_service = iam
        .rule_service
        .as_ref()
        .ok_or_else(|| Error::business_error("互斥规则服务未初始化".to_string()))?;

    let affected = rule_service
        .remove_rule_items(&svr_ctx, &req.rule_id, &req.item_ids)
        .await
        .map_err(|e| Error::business_error(e.to_string()))?;

    Ok(Json(ApiResp::ok(BatchResponse { affected })))
}

/// 分页查询规则
#[utoipa::path(
    post,
    path = "/api/iam/exclusion-rules/page",
    request_body = PageRulesRequest,
    responses(
        (status = 200, description = "查询成功", body = ApiResp<PageRulesResponse>)
    ),
    tag = "IAM-Exclusion"
)]
pub async fn page_rules(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Json(req): Json<PageRulesRequest>,
) -> Result<Json<ApiResp<PageRulesResponse>>> {
    debug!(
        "{:<12} - handler::page_rules - current: {}, size: {}",
        "HANDLER", req.current, req.size
    );

    let iam = cmx_state
        .iam()
        .ok_or_else(|| Error::business_error("IAM 服务未初始化".to_string()))?;

    let rule_service = iam
        .rule_service
        .as_ref()
        .ok_or_else(|| Error::business_error("互斥规则服务未初始化".to_string()))?;

    let (rules, total) = rule_service
        .page_rules(req.current, req.size)
        .await
        .map_err(|e| Error::business_error(e.to_string()))?;

    Ok(Json(ApiResp::ok(PageRulesResponse { rules, total })))
}

/// 规则校验测试
#[utoipa::path(
    post,
    path = "/api/iam/exclusion-rules/validate",
    request_body = ValidateRuleRequest,
    responses(
        (status = 200, description = "校验完成", body = ApiResp<ValidateRuleResponse>)
    ),
    tag = "IAM-Exclusion"
)]
pub async fn validate_rule(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    Json(req): Json<ValidateRuleRequest>,
) -> Result<Json<ApiResp<ValidateRuleResponse>>> {
    debug!(
        "{:<12} - handler::validate_rule - perm_count: {}, role_count: {}, user: {:?}",
        "HANDLER", req.permission_ids.len(), req.role_ids.len(), req.user_id
    );

    let iam = cmx_state
        .iam()
        .ok_or_else(|| Error::business_error("IAM 服务未初始化".to_string()))?;

    let rule_service = iam
        .rule_service
        .as_ref()
        .ok_or_else(|| Error::business_error("互斥规则服务未初始化".to_string()))?;

    let response = rule_service
        .validate_rule(req)
        .await
        .map_err(|e| Error::business_error(e.to_string()))?;

    Ok(Json(ApiResp::ok(response)))
}
