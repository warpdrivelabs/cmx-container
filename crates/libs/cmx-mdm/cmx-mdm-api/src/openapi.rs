//! cmx-mdm-api 的 OpenApi 切片。
//!
//! 主数据（MDM）域的 paths（handler 的 `#[utoipa::path]` 注解），由 platform-app
//! 用 `OpenApi::merge()` 聚合到主文档。响应 schema（`ApiResp<Value>` 等）由 utoipa
//! 从各 path 的 `body=` 自动收集，无需在此显式声明 components。

use utoipa::OpenApi;

/// 主数据（MDM）OpenApi 切片。
#[derive(OpenApi)]
#[openapi(
    paths(
        // health
        crate::handlers::mdm_health,
        // activation（激活映射配置 CRUD + 手动激活）
        crate::handlers::mdm_activations_list,
        crate::handlers::mdm_activations_save,
        crate::handlers::mdm_activations_delete,
        crate::handlers::mdm_cr_activate,
        // change-requests（CR 变更请求审批流转 / 列表 / 详情）
        crate::handlers::mdm_cr_submit,
        crate::handlers::mdm_cr_approve,
        crate::handlers::mdm_cr_reject,
        crate::handlers::mdm_cr_abort,
        crate::handlers::mdm_cr_list,
        crate::handlers::mdm_cr_detail,
        // dedup（实时查重 + 关键信息查重）
        crate::handlers::mdm_find_duplicates,
        crate::handlers::mdm_check_key,
        // merge-requests（合并请求确认 / 列表 / 详情 / 驳回 / 还原）
        crate::handlers::mdm_merge_requests_list,
        crate::handlers::mdm_merge_requests_create,
        crate::handlers::mdm_merge_request_detail,
        crate::handlers::mdm_merge_request_reject,
        crate::handlers::mdm_merge_requests_undo,
        // governance（审计 / 事件 / 订阅 / 发布）
        crate::handlers::mdm_audit_list,
        crate::handlers::mdm_events_list,
        crate::handlers::mdm_subscriptions_list,
        crate::handlers::mdm_subscriptions_save,
        crate::handlers::mdm_publish,
        // match-configs（查重规则配置）
        crate::handlers::mdm_match_configs_list,
        crate::handlers::mdm_match_configs_save,
        crate::handlers::mdm_match_configs_delete,
        // match-scan（全库扫描查重）
        crate::handlers::mdm_match_scan_run,
        crate::handlers::mdm_match_scan_list,
        crate::handlers::mdm_match_scan_detail,
        crate::handlers::mdm_match_scan_ignore,
        // workbench（管家工作台汇总）
        crate::handlers::mdm_workbench_summary,
    )
)]
pub struct MdmApiDoc;
