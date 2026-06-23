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

/// 启用/禁用规则请求
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct ToggleRuleStatusRequest {
    pub rule_id: String,
    pub status: i64, // 0-禁用，1-启用
}

/// 添加规则项请求
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct AddRuleItemsRequest {
    pub rule_id: String,
    pub subject_ids: Vec<String>,
}

/// 移除规则项请求
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct RemoveRuleItemsRequest {
    pub rule_id: String,
    pub item_ids: Vec<String>,
}

/// 规则详情响应（含规则项）
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct RuleDetailResponse {
    pub rule: ExclusionRule,
    pub items: Vec<ExclusionRuleItem>,
}

/// 批量操作响应
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct BatchResponse {
    pub affected: u64,
}

/// 分页查询规则请求
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct PageRulesRequest {
    pub current: u64,
    pub size: u64,
}

/// 分页查询规则响应
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct PageRulesResponse {
    pub rules: Vec<ExclusionRule>,
    pub total: i64,
}

/// 创建规则
#[utoipa::path(
    post,
    path = "/api/iam/exclusion-rules/create",
    request_body = CreateExclusionRuleRequest,
    responses(
        (status = 200, description = "创建成功")
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
        (status = 200, description = "更新成功")
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
        (status = 200, description = "删除成功")
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
        (status = 200, description = "查询成功")
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
        (status = 200, description = "切换成功")
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
        (status = 200, description = "添加成功")
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
        (status = 200, description = "移除成功")
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
        (status = 200, description = "查询成功")
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
        (status = 200, description = "校验完成")
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
