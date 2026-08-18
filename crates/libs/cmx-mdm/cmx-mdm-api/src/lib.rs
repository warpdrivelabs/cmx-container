//! cmx-mdm-api —— 主数据（MDM）模块的 HTTP 层。
//!
//! 实现 cmx-api 的 [`ModuleRoutes`]，由 web-server 合并进主路由（`/mdm/*`，`/api` 前缀由
//! web-server nest 加）。cmx-api 不反向依赖本 crate（无环）。
//!
//! **API 约定**（承接 AGENTS.md §四 第 5 条）：新增接口禁用 Path Variable，资源标识/参数
//! 走 query（GET）或 JSON body（POST 等）。
//!
//! M0 仅一个健康检查端点；M1+ 追加激活映射配置、变更请求激活等治理端点。

/// MDM 模块全部 axum handler 的实现集合（按业务域分文件组织）。
pub mod handlers;

/// M7 流程平台客户端（回环调本进程 `/api/flow/*`，双部署模式透明）。
pub mod flow_client;

/// OpenApi 切片（MdmApiDoc），由 platform-app `merged_openapi()` 合并进主文档。
pub mod openapi;

pub use openapi::MdmApiDoc;

use axum::Router;
use axum::routing::{get, post};

use cmx_api_core::CmxAppState;
use cmx_api_core::routes::traits::ModuleRoutes;

use handlers as mdm;

/// MDM 模块路由聚合（实现 cmx-api 的 ModuleRoutes，由 web-server 合并进主路由）。
pub struct MdmModule;

impl ModuleRoutes for MdmModule {
    fn routes(self) -> Router<CmxAppState> {
        Router::new()
            // 健康检查
            .route("/mdm/health", get(mdm::mdm_health))
            // 激活映射配置 CRUD（配置器 UI 用；GET 列表 + POST 保存 + POST 删除）
            .route(
                "/mdm/activations",
                get(mdm::mdm_activations_list).post(mdm::mdm_activations_save),
            )
            .route("/mdm/activations/delete", post(mdm::mdm_activations_delete))
            // 手动触发激活（body: { crId }；禁用 Path Variable，承接 AGENTS.md §四 第 5 条）
            .route("/mdm/change-requests/activate", post(mdm::mdm_cr_activate))
            // M2 · CR 变更请求:审批流转/列表/详情（新建走标准 /doc/save）
            .route("/mdm/change-requests/submit", post(mdm::mdm_cr_submit))
            // M7.1 决议：approve/reject 旧端点删除（与 review 封装重叠），activate 保留兜底
            .route("/mdm/change-requests/abort", post(mdm::mdm_cr_abort))
            .route("/mdm/change-requests", get(mdm::mdm_cr_list))
            .route("/mdm/change-requests/detail", get(mdm::mdm_cr_detail))
            // M7 · 流程平台对接：webhook 回调（免用户鉴权路径，HMAC 签名即凭证）+
            // 撤回（发起人 cancel+回草稿）+ 流程状态懒同步 + 审批历史
            .route("/mdm/flow/callback", post(mdm::flow_cb::mdm_flow_callback))
            .route("/mdm/change-requests/withdraw", post(mdm::flow_cb::mdm_cr_withdraw))
            .route("/mdm/change-requests/flow-status", get(mdm::flow_cb::mdm_cr_flow_status))
            .route("/mdm/change-requests/flow-history", get(mdm::flow_cb::mdm_cr_flow_history))
            // M7.1 审批动作业务封装（前端只传 crId+action+comment，流程调用全在 MDM 内）
            .route("/mdm/change-requests/review", post(mdm::review::mdm_cr_review))
            .route("/mdm/change-requests/return", post(mdm::review::mdm_cr_return))
            .route("/mdm/change-requests/review-context", get(mdm::review::mdm_cr_review_context))
            // M3 · 匹配合并（禁用 Path Variable，参数走 body/query）
            .route("/mdm/records/find-duplicates", post(mdm::mdm_find_duplicates))
            // V3.2 · 步骤条关键信息查重（新建场景，无 recordId）
            .route("/mdm/check-key", post(mdm::mdm_check_key))
            .route(
                "/mdm/merge-requests",
                get(mdm::mdm_merge_requests_list).post(mdm::mdm_merge_requests_create),
            )
            .route("/mdm/merge-requests/undo", post(mdm::mdm_merge_requests_undo))
            // MDM 治理端点（分页 + 无 path variable）
            .route("/mdm/audit", get(mdm::mdm_audit_list))
            .route("/mdm/events", get(mdm::mdm_events_list))
            .route(
                "/mdm/subscriptions",
                get(mdm::mdm_subscriptions_list).post(mdm::mdm_subscriptions_save),
            )
            .route("/mdm/publish", post(mdm::mdm_publish))
            // M4 · 管家工作台：详情（红线 diff）/ 驳回
            .route("/mdm/merge-requests/detail", get(mdm::mdm_merge_request_detail))
            .route("/mdm/merge-requests/reject", post(mdm::mdm_merge_request_reject))
            // 查重规则配置（规则维护内嵌查重界面，无独立管理页）
            .route("/mdm/match-configs", get(mdm::mdm_match_configs_list).post(mdm::mdm_match_configs_save))
            .route("/mdm/match-configs/delete", post(mdm::mdm_match_configs_delete))
            // M3.5 · 全库扫描查重（管家工作台「发现未知重复」入口）
            .route("/mdm/match-scan", get(mdm::mdm_match_scan_list))
            .route("/mdm/match-scan/run", post(mdm::mdm_match_scan_run))
            .route("/mdm/match-scan/detail", get(mdm::mdm_match_scan_detail))
            .route("/mdm/match-scan/ignore", post(mdm::mdm_match_scan_ignore))
            // M4 · 管家工作台：汇总计数（发现项 + 合并历史各状态数量）
            .route("/mdm/workbench/summary", get(mdm::mdm_workbench_summary))
    }

    fn prefix() -> &'static str {
        "mdm"
    }

    fn module_name(&self) -> &'static str {
        "mdm"
    }
}
