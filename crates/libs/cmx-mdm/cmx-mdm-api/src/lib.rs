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

/// M5 分发订阅引擎（通道注册表 + Webhook 通道 + Dispatcher 常驻循环）。
pub mod distribution;

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
        health_routes()
            .merge(activation_routes())
            .merge(cr_routes())
            .merge(flow_routes())
            .merge(dedup_routes())
            .merge(merge_routes())
            .merge(subscription_routes())
            .merge(distribution_routes())
    }

    fn prefix() -> &'static str {
        "mdm"
    }

    fn module_name(&self) -> &'static str {
        "mdm"
    }
}

// ── 分组路由（与 handlers/ 子模块业务域一一对应，各组路径互不重叠）─────────

/// 健康检查。
fn health_routes() -> Router<CmxAppState> {
    Router::new()
        // 模块存活探针：GET 无参，返回固定 `{ module, status }`
        .route("/mdm/health", get(mdm::mdm_health))
}

/// 激活映射配置（M1 配置器 UI）+ 手动激活。
fn activation_routes() -> Router<CmxAppState> {
    Router::new()
        // 激活映射配置列表 + 保存（GET 分页，配置器数据源 / POST：有 id 更新、无 id 新增）
        .route(
            "/mdm/activations",
            get(mdm::mdm_activations_list).post(mdm::mdm_activations_save),
        )
        // 删除激活映射配置（POST body 传 id）
        .route("/mdm/activations/delete", post(mdm::mdm_activations_delete))
        // 手动触发激活（POST body `{ crId }`：按映射把 CR 数据落到主数据表并写事件）
        .route("/mdm/change-requests/activate", post(mdm::mdm_cr_activate))
}

/// CR 变更请求（M2 审批流转 / 列表 / 详情；新建走标准 /doc/save）。
fn cr_routes() -> Router<CmxAppState> {
    Router::new()
        // 提交 CR 送审（POST：进入审批流，走流程平台发起流程实例）
        .route("/mdm/change-requests/submit", post(mdm::mdm_cr_submit))
        // 作废 CR（POST：终止审批并留痕；M7.1 起 approve/reject 旧端点已删，走 review 封装）
        .route("/mdm/change-requests/abort", post(mdm::mdm_cr_abort))
        // CR 列表（GET 分页 + 状态过滤）
        .route("/mdm/change-requests", get(mdm::mdm_cr_list))
        // CR 详情（GET ?id=：单据头行 + 激活红线 diff）
        .route("/mdm/change-requests/detail", get(mdm::mdm_cr_detail))
}

/// 流程平台对接（M7 webhook 回调 + 回写状态机）+ M7.1 审批动作业务封装。
fn flow_routes() -> Router<CmxAppState> {
    Router::new()
        // 流程 webhook 回调（POST：免用户鉴权路径，HMAC 签名即凭证，回写 CR 状态机）
        .route("/mdm/flow/callback", post(mdm::flow_cb::mdm_flow_callback))
        // 撤回 CR（POST：发起人撤回，cancel 流程实例并回草稿态）
        .route("/mdm/change-requests/withdraw", post(mdm::flow_cb::mdm_cr_withdraw))
        // 流程实例状态查询（GET：懒同步——以流程平台为准刷新本地 CR 状态）
        .route("/mdm/change-requests/flow-status", get(mdm::flow_cb::mdm_cr_flow_status))
        // 审批历史（GET：流程 transcript 映射为审批意见时间线）
        .route("/mdm/change-requests/flow-history", get(mdm::flow_cb::mdm_cr_flow_history))
        // 审批动作封装（POST：同意/驳回，前端只传 crId+action+comment，流程调用全在 MDM 内）
        .route("/mdm/change-requests/review", post(mdm::review::mdm_cr_review))
        // 退回（POST：退给发起人修改后重新提交）
        .route("/mdm/change-requests/return", post(mdm::review::mdm_cr_return))
        // 审批上下文（GET：审批详情页按钮显隐与单据摘要数据源）
        .route("/mdm/change-requests/review-context", get(mdm::review::mdm_cr_review_context))
}

/// 查重（M3 实时查重 + V3.2 关键信息查重）+ 查重规则配置维护。
fn dedup_routes() -> Router<CmxAppState> {
    Router::new()
        // 实时查重（POST：按表单值/记录找重复候选，供录入页提示）
        .route("/mdm/records/find-duplicates", post(mdm::mdm_find_duplicates))
        // 关键信息查重（POST：新建场景步骤条预校验，无 recordId）
        .route("/mdm/check-key", post(mdm::mdm_check_key))
        // 查重规则配置列表 + 保存（GET 分页 / POST：有 id 更新；规则维护内嵌查重界面，无独立管理页）
        .route(
            "/mdm/match-configs",
            get(mdm::mdm_match_configs_list).post(mdm::mdm_match_configs_save),
        )
        // 删除查重规则配置（POST body 传 id）
        .route("/mdm/match-configs/delete", post(mdm::mdm_match_configs_delete))
}

/// 合并请求（M3 确认/还原 + M4 管家工作台详情/驳回）+ M3.5 全库扫描查重 + 汇总计数。
fn merge_routes() -> Router<CmxAppState> {
    Router::new()
        // 合并请求列表 + 创建（GET 分页 / POST：确认合并，胜出方吸收其余记录产生 golden record）
        .route(
            "/mdm/merge-requests",
            get(mdm::mdm_merge_requests_list).post(mdm::mdm_merge_requests_create),
        )
        // 还原合并（POST：撤销已完成的合并，恢复各源记录）
        .route("/mdm/merge-requests/undo", post(mdm::mdm_merge_requests_undo))
        // 合并详情（GET ?id=：字段级红线 diff，管家工作台审核用）
        .route("/mdm/merge-requests/detail", get(mdm::mdm_merge_request_detail))
        // 驳回合并请求（POST：终态留痕）
        .route("/mdm/merge-requests/reject", post(mdm::mdm_merge_request_reject))
        // 扫描任务列表（GET 分页）
        .route("/mdm/match-scan", get(mdm::mdm_match_scan_list))
        // 发起全库扫描查重（POST：管家工作台「发现未知重复」入口，异步产出入组）
        .route("/mdm/match-scan/run", post(mdm::mdm_match_scan_run))
        // 扫描详情（GET ?id=：重复组明细与命中字段得分）
        .route("/mdm/match-scan/detail", get(mdm::mdm_match_scan_detail))
        // 忽略重复组（POST：人工判定非重复，不再提示）
        .route("/mdm/match-scan/ignore", post(mdm::mdm_match_scan_ignore))
        // 管家工作台汇总计数（GET：发现项 + 合并历史各状态数量）
        .route("/mdm/workbench/summary", get(mdm::mdm_workbench_summary))
}

/// 订阅与治理（M5 订阅 CRUD / 启停 / 测试 + 审计 / 事件日志 / 手动补发）。
fn subscription_routes() -> Router<CmxAppState> {
    Router::new()
        // 变更审计列表（GET 分页：who/when/field 级新旧值留痕）
        .route("/mdm/audit", get(mdm::mdm_audit_list))
        // 事件日志列表（GET 分页：delta 拉取，since 游标；order=desc 时最新在前供监控页）
        .route("/mdm/events", get(mdm::mdm_events_list))
        // 订阅列表 + 保存（GET 分页含近 24h 投递统计 / POST：有 id 更新、无 id 新增，secret 掩码回显）
        .route(
            "/mdm/subscriptions",
            get(mdm::mdm_subscriptions_list).post(mdm::mdm_subscriptions_save),
        )
        // 删除订阅（POST body 传 id）
        .route("/mdm/subscriptions/delete", post(mdm::mdm_subscriptions_delete))
        // 启用/停用订阅（POST body：id + active）
        .route("/mdm/subscriptions/set-active", post(mdm::mdm_subscriptions_set_active))
        // 订阅连通性测试（POST：按当前配置发一条试探投递，回显响应码/耗时）
        .route("/mdm/subscriptions/test", post(mdm::mdm_subscriptions_test))
        // 可用分发通道列表（GET：通道注册表元信息）
        .route("/mdm/subscriptions/channels", get(mdm::mdm_subscriptions_channels))
        // 手动补发（POST：按订阅/字典 + seq 范围重建待投递实例，上限 5000 行）
        .route("/mdm/publish", post(mdm::mdm_publish))
}

/// 分发投递治理（M5 投递流水 / 统计 / 重发 / 跳过 + pull 游标 + 全量快照）。
fn distribution_routes() -> Router<CmxAppState> {
    Router::new()
        // 投递流水查询（POST body：多过滤 + 分页，created_at 倒序）
        .route("/mdm/dispatches/query", post(mdm::mdm_dispatches_query))
        // 单条投递详情（GET ?id=：投递全列 + 事件类型/payload + 订阅名）
        .route("/mdm/dispatches/detail", get(mdm::mdm_dispatches_detail))
        // 重发（POST body：ids 列表或订阅+状态批量，failed/dead → pending）
        .route("/mdm/dispatches/retry", post(mdm::mdm_dispatches_retry))
        // 人工跳过死信（POST body：ids，终态 skipped 决策留痕）
        .route("/mdm/dispatches/skip", post(mdm::mdm_dispatches_skip))
        // 监控 KPI 统计（GET：今日投递/成功率/平均耗时/积压/死信/扇出滞后）
        .route("/mdm/dispatches/stats", get(mdm::mdm_dispatches_stats))
        // pull 游标登记（POST：consumerId+dictCode+seq，单调递增仅接受更大值）
        .route("/mdm/events/ack", post(mdm::mdm_events_ack))
        // pull 消费者游标列表（GET：消费进度与 lag，监控页直显）
        .route("/mdm/events/offsets", get(mdm::mdm_events_offsets))
        // 全量快照分页拉取（POST：首接/对账修复，支持按日期段增量）
        .route("/mdm/records/snapshot", post(mdm::mdm_records_snapshot))
}
