# cmx-mdm-api

> 主数据管理（MDM）模块的 HTTP 协议皮肤：薄 axum handler 集合 + `MdmModule`（实现 `cmx-api-core` 的 `ModuleRoutes`）路由聚合 + M7 流程平台回环客户端 + M5 分发订阅引擎，由 web-server 合并进主路由（`/api/mdm/*`）。

[![Version](https://img.shields.io/badge/version-0.1.12-blue.svg)]()
[![Edition](https://img.shields.io/badge/rust--edition-2024-orange.svg)]()

---

## 项目简介

`cmx-mdm-api` 是 CMX 平台主数据管理（MDM）模块的 HTTP 层。它遵循平台「域三件套」分层：
本 crate 只做**协议适配**（参数提取 → 调 store/model 层 → `ApiResp` 信封封装），业务语义
沉淀在 `cmx-mdm-model`（纯逻辑）与 `cmx-mdm-store-pg`（PostgreSQL 持久化）中。

### 核心业务概念

- **CR 变更请求（Change Request）**：主数据的一切变更（新建/修改）都以 CR 单据
  （`cv_mdm_apply` 头 + `cv_mdm_apply_line` 行）发起，新建走平台标准 `/doc/save`，
  审批通过后由激活器（store-pg 的 `activation_service::activate`）落库为 `cm_*` 主数据。
- **激活映射配置（`mdm_activation`）**：声明「源单据字段 → 主数据列」的搬运规则，
  配置器 UI 经本 crate 的 `/mdm/activations` 端点维护。
- **匹配 / 合并 / 存活**：M3 查重（`/mdm/records/find-duplicates`、`/mdm/check-key`）、
  合并请求（`/mdm/merge-requests/*`）、全库扫描（`/mdm/match-scan/*`）与管家工作台
  （`/mdm/workbench/summary`）。
- **分发订阅（M5）**：主数据变更事件（`md_event_log`）按订阅（`md_subscription`）
  扇出为投递实例（`md_dispatch_log`），经通道（webhook，可选 kafka/rocketmq）推送下游，
  由本 crate 内的 Dispatcher 常驻循环异步执行。
- **流程平台对接（M7）**：CR 提交后回环调用本进程 `/api/flow/*`（cmx-flowengine 协议）
  发起审批实例；流程侧经 `/mdm/flow/callback` webhook（HMAC-SHA256 验签）回写结果。

### API 约定

承接 `AGENTS.md` §四 第 5 条：**新增接口禁用 Path Variable**，资源标识/参数一律走
query（GET）或 JSON body（POST 等）。全部端点挂 `/api` 前缀（由 web-server nest 加）。

---

## 与其他 crate 的关系

### 上游依赖（本 crate 依赖）

| 依赖 | 用途 |
|------|------|
| `cmx-mdm-model` | MDM 中立模型（`ActivationConfig` / `EventEnvelope` / `DistributionChannel` trait / 匹配算法类型） |
| `cmx-mdm-store-pg` | MDM 持久化/服务层（激活器主流程 `activate`/`merge`/`unmerge`、各 store 自由函数） |
| `cmx-api-core` | API 框架：`CmxAppState` / `middleware::CmxSvrContext` / `ModuleRoutes` trait / `ApiResp`/`Result`/`Error` / `resolve_db_id_from_headers` |
| `cmx-api-types` | 统一响应信封 `ApiResp<T>`（OpenApi schema 引用） |
| `cmx-biz` | 业务错误码（`Violation`：校验失败结构化响应） |
| `cmx-database-pg` | tokio-postgres 并行 DB 层（`get_default_pg_db_manager` 全局单例） |
| `cmx-dct-store-pg` | DCT 字典解析（`dict_meta` 取表名/列清单，替代 merge handler 硬编码） |
| `cmx-portal` | 分发死信/连续失败门户通知（cmx-portal 不反向依赖 cmx-mdm-*，无环） |
| `cmx-core` / `cmx-utils` / `cmx-traits` | `DataValue` 参数构造 / `[mdm.flow]` 配置读取 / `current_original_token` |
| `axum` / `utoipa` / `reqwest` | Web 框架（handler 提取器 + Router）/ OpenAPI 文档 / M7 回环 HTTP 客户端 |
| `hmac` / `sha2` / `hex` / `dashmap` / `rand` / `tokio` / `async-trait` | webhook 验签 / 通道注册表 / 订阅 secret 生成 / dispatcher 常驻循环 |

### 下游使用方（谁依赖本 crate）

| 使用方 | 引用方式 | 实际用途 |
|--------|---------|---------|
| `cmx-platform-app` | `cmx-mdm-api = { workspace = true }` | `routes.rs` 中 `.merge(MdmModule.routes())` 合并约 40 个 MDM 端点；`merged_openapi()` 中 `.merge(MdmApiDoc::openapi())` 合并 OpenAPI 切片；启动链调用 `start_distribution()` 拉起分发循环 |
| `cmx-portalservice`（跨 workspace） | 经 `cmx-platform-app` path 引用间接依赖 | 门户后端可执行进程（cmx-portal-server）承载全部 MDM HTTP 端点 |
| `cmx-api` / `web-server` | **不依赖** | cmx-api 不反向依赖本 crate（避免环）；路由合并统一发生在 cmx-platform-app |

```text
cmx-api-core（ModuleRoutes trait）    cmx-flowengine（/api/flow/* 协议）
        ▲ 接口实现                          ▲ 回环 HTTP + webhook 回调
        │                                   │
┌───────┴───────────────────────────────────┴────────┐
│ cmx-mdm-api（本 crate：handlers + flow_client +    │
│             distribution + openapi + MdmModule）   │
└───────┬──────────────────────────┬─────────────────┘
        ▼                          ▼
  cmx-mdm-model               cmx-mdm-store-pg
  （纯逻辑/契约）              （PostgreSQL 持久化）
```

---

## 核心功能与特性

| 功能域 | 端点（前缀 `/api/mdm`） | 说明 |
|--------|------------------------|------|
| 健康检查 | `GET /mdm/health` | 模块存活探测 |
| 激活映射配置 | `/mdm/activations`（GET 列表 / POST 保存）、`/mdm/activations/delete` | 配置器 UI 维护 `mdm_activation`（upsert by activationCode） |
| CR 审批流转 | `/mdm/change-requests/submit`、`/abort`、`/withdraw` | 抢占式状态迁移 + 回环发起流程实例（六步：抢占→防孤儿→发起→信封判据→代确认） |
| CR 查询 | `GET /mdm/change-requests`、`/detail` | 按 docStatus/docType/keyword 过滤分页、四层主从详情 |
| 审批动作封装 | `/mdm/change-requests/review`、`/return`、`/review-context` | 前端只传 crId+action+comment，流程定位/权限判定/激活触发全在 MDM 内 |
| 流程对接 | `/mdm/flow/callback`、`/flow-status`、`/flow-history` | webhook 回调（HMAC-SHA256 验签即凭证）、状态懒同步、审批历史 |
| 手动激活兜底 | `POST /mdm/change-requests/activate` | 受 `[mdm.flow].manual_override_enabled` 开关保护（默认 403） |
| 查重 | `/mdm/records/find-duplicates`、`/mdm/check-key` | 锚点查重 + 新建步骤条关键信息查重（加权分 ≥80 阻断） |
| 匹配合并 | `/mdm/merge-requests`（GET/POST）、`/undo`、`/detail`、`/reject` | 合并请求生命周期 + 管家工作台红线 diff 详情 |
| 查重配置 | `/mdm/match-configs`（GET/POST）、`/delete` | `md_match_config` 查重规则维护（内嵌查重界面） |
| 全库扫描 | `/mdm/match-scan`（list/run/detail/ignore） | M3.5 普查模式查重，发现项落 `md_match_scan` 供管家评审 |
| 管家工作台 | `GET /mdm/workbench/summary` | 发现项 + 合并历史各状态汇总计数 |
| 治理审计 | `GET /mdm/audit`、`/mdm/events` | `md_audit` / `md_event_log` 分页查询 |
| 分发订阅 | `/mdm/subscriptions`（list/save/delete/set-active/test/channels） | 订阅 CRUD + 通道连通性测试（test 信封不落库） |
| 分发治理 | `/mdm/dispatches`（query/detail/retry/skip/stats）、`/mdm/events/ack`、`/events/offsets`、`/records/snapshot` | 投递流水、重发/跳过、pull 游标、全量快照 |
| 手动发布 | `POST /mdm/publish` | 手工补发主数据事件 |
| OpenAPI | `MdmApiDoc::openapi()` | utoipa 切片，由 platform-app 合并进主文档 |

---

## 模块结构

```text
cmx-mdm-api
├── src
│   ├── lib.rs                     # MdmModule（impl ModuleRoutes）：约 40 条路由聚合
│   ├── handlers/                  # 全部 axum handler（按业务域分文件）
│   │   ├── mod.rs                 #   公共 DTO（SpecDto）+ 分页默认值 + re-export
│   │   ├── activation.rs          #   激活映射配置 CRUD + 手动激活兜底端点
│   │   ├── cr.rs                  #   CR 提交（六步抢占流）/ 作废 / 列表 / 详情
│   │   ├── review.rs              #   审批动作封装（review / return / review-context）
│   │   ├── flow_cb.rs             #   流程 webhook 回调 + 撤回 + 懒同步 + manual_override_guard
│   │   ├── dedup.rs               #   查重（find-duplicates / check-key）+ DictMeta 解析
│   │   ├── merge.rs               #   合并请求生命周期（创建/列表/详情/驳回/撤销）
│   │   ├── match_config.rs        #   查重规则配置 CRUD
│   │   ├── scan.rs                #   全库扫描查重（run/list/detail/ignore）
│   │   ├── workbench.rs           #   管家工作台汇总计数
│   │   ├── governance.rs          #   审计/事件/订阅治理 + 手动 publish
│   │   └── distribution.rs        #   分发治理（dispatches/offsets/snapshot）
│   ├── flow_client.rs             # M7 流程平台回环客户端（FlowCfg + start_instance 等 12 个函数）
│   ├── distribution/              # M5 分发订阅引擎
│   │   ├── mod.rs                 #   DistCfg（[mdm.distribution] 配置）+ start_distribution
│   │   ├── registry.rs            #   ChannelRegistry：并发只读通道注册表（global 单例）
│   │   ├── dispatcher.rs          #   常驻循环：interval 扫描 + 扇出 + 并发投递 + 退避重试
│   │   ├── transform.rs           #   订阅级过滤（event_matches_sub）+ field_map 投影 + build_envelope
│   │   └── channels/              #   通道实现：webhook（默认）/ kafka / rocketmq（feature 门控）
│   └── openapi.rs                 # MdmApiDoc（utoipa OpenApi 切片）
└── Cargo.toml
```

---

## 关键类型 / API

### 路由聚合（lib.rs）

```rust
pub struct MdmModule;

impl ModuleRoutes for MdmModule {
    fn routes(self) -> Router<CmxAppState> { /* 约 40 条 .route(...) */ }
    fn prefix() -> &'static str { "mdm" }
    fn module_name(&self) -> &'static str { "mdm" }
}
```

### 流程对接（flow_client.rs）

| 项 | 签名 / 说明 |
|----|------------|
| `FlowCfg` | `[mdm.flow]` 配置快照：`loopback_base`（默认 `http://127.0.0.1:8080`）、`definition_key`（默认 `mdm_cr_approval`）、`webhook_secret`、`timeout_ms`、`manual_override_enabled`（默认 false） |
| `flow_cfg() -> FlowCfg` | 读配置段（缺项回退默认值） |
| `start_instance(head, cr_id, user_token)` | 回环 `POST /api/flow/instances` 发起审批实例（bizLink 绑 `cv_mdm_apply`） |
| `complete_apply_task / complete_review_task / return_review_task` | 以指定身份 complete/回退流程任务（委托令牌透传） |
| `biz_instances(cr_id) -> Result<Vec<Value>, String>` | 按 bizLink 反查 CR 关联的流程实例 |
| `instance_detail / instance_variables / instance_comments / cancel_instance / my_claimable_tasks` | 实例详情/变量/意见/取消/可认领任务查询 |
| `MDM_BIZ_TABLE: &str = "cv_mdm_apply"` | MDM 单据绑定的流程表名（biz_link 坐标） |

### 分发引擎（distribution/）

| 项 | 签名 / 说明 |
|----|------------|
| `DistCfg` | `[mdm.distribution]` 配置：`enabled`、`scan_interval_ms`（默认 2000）、`fanout_batch`（500）、`deliver_batch`（100）、`deliver_concurrency`（8）、`backoff_base_ms`（5000）、`backoff_max_ms`（1_800_000）、`running_reclaim_minutes`（10）、`allow_private_address`（true） |
| `dist_cfg() -> DistCfg` | 读配置段（进程内不缓存） |
| `start_distribution() -> Result<(), String>` | 引擎入口：注册通道 + 按配置拉起 Dispatcher 循环（幂等；`enabled=false` 只注册通道不 spawn） |
| `ChannelRegistry::global()` | 并发只读通道注册表（dashmap）；新通道 = 实现 `DistributionChannel` trait + `register(Arc::new(...))` |
| `dispatcher::run(cfg: DistCfg)` | 常驻循环（由 start_distribution spawn） |
| `transform::event_matches_sub / apply_field_map / build_envelope` | 订阅过滤 / 字段投影（裁剪·重命名·脱敏）/ 构造 `EventEnvelope` |
| `channels::WebhookChannel` | 默认通道：HMAC-SHA256 签名投递 + 私网地址 SSRF 策略 |

### 代表性 handler（handlers/）

| 项 | 说明 |
|----|------|
| `mdm_cr_submit` | 六步提交：`try_set_cr_status_pub`（draft/rejected→approving 抢占）→ 读头 → 防孤儿实例 → `start_instance`（信封判据：HTTP 2xx 且 code==0）→ 失败回滚 → 代确认 apply 节点 |
| `mdm_cr_review` | 审批统一入口：`locate_review_task`（定位 ACTIVE 实例 + 未办结 review 任务 + assignee/候选池权限）→ approve 走激活、reject 回 rejected |
| `mdm_flow_callback` | webhook 回调：HMAC 验签即凭证（免用户鉴权路径）→ `sync_flow_result_with` 收敛 CR 状态 |
| `manual_override_guard() -> Result<()>` | 手动激活开关闸（`manual_override_enabled=false` 时返回 403） |
| `SpecDto` | 查重字段规则 DTO（field / weight / kind），与 `ActivationConfig::key_fields` 语义一致 |

---

## 使用示例

### 一、组装层合并路由 + 拉起分发引擎（cmx-platform-app 场景）

```rust
use axum::Router;
use cmx_api_core::CmxAppState;
use cmx_mdm_api::{MdmModule, MdmApiDoc};

fn mdm_routes() -> Router<CmxAppState> {
    // MdmModule 实现 ModuleRoutes：合并后端点挂 /mdm/*（/api 前缀由外层 nest 加）
    MdmModule.routes()
}

fn mdm_openapi() -> utoipa::openapi::OpenApi {
    // OpenApi 切片合并进平台主文档（merged_openapi）
    MdmApiDoc::openapi()
}

// 启动链中拉起分发引擎（幂等；enabled=false 时仅注册通道实现不 spawn 循环）：
// cmx_mdm_api::distribution::start_distribution().ok();
```

### 二、提交 CR 并推进审批（前端调用序列）

```text
1. POST /api/doc/save               # 新建 CR 单据（平台标准单据保存，非本 crate 端点）
2. POST /api/mdm/change-requests/submit   body: { "crId": 123 }
   → { "crId": 123, "status": "approving", "instanceId": "..." }
   # 内部：抢占 draft/rejected→approving（并发双击 409）→ 回环发起流程 → 代确认 apply 节点
3. POST /api/mdm/change-requests/review   body: { "crId": 123, "action": "approve", "comment": "同意" }
   → { "crId": 123, "action": "approve", "status": "activated", "instanceId": "..." }
   # approve 内部触发激活器（store-pg activate 七步单事务），cm_* 落库为 published
4. GET  /api/mdm/change-requests/detail?crId=123   # 四层主从详情（头/行/流程/审计）
```

### 三、新增一个分发通道（扩展点）

```rust
use std::sync::Arc;
use async_trait::async_trait;
use cmx_mdm_model::distribution::{DistributionChannel, EventEnvelope, DeliveryResult};
use cmx_mdm_api::distribution::registry::ChannelRegistry;
use serde_json::Value;

/// 自定义通道：实现 model 层的 DistributionChannel trait。
struct MyChannel;

#[async_trait]
impl DistributionChannel for MyChannel {
    fn channel_type(&self) -> &'static str { "my_channel" }

    async fn validate_config(&self, config: &Value) -> Result<(), String> {
        // 保存订阅时前置校验 channel_config 结构（错误信息直接回显前端）
        config.get("endpoint").and_then(|v| v.as_str())
            .map(|_| ())
            .ok_or_else(|| "缺 endpoint".to_string())
    }

    async fn deliver(&self, config: &Value, envelopes: &[EventEnvelope])
        -> Vec<DeliveryResult>
    {
        // 逐条投递、逐条返回；不得因单条失败中断整批
        envelopes.iter().map(|e| DeliveryResult::ok(&e.event_id, None, None)).collect()
    }

    async fn health_check(&self, config: &Value) -> Result<(), String> {
        Ok(()) // 订阅「测试」按钮的连通性探测
    }
}

// 启动链注册（引擎与 store 零改动；/mdm/subscriptions/channels 自动列出）：
// ChannelRegistry::global().register(Arc::new(MyChannel));
```

### 四、handler 内调用 store 层（本 crate 惯例）

```rust
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::Json;
use cmx_api_core::db_id::resolve_db_id_from_headers;
use cmx_api_core::middleware::CmxSvrContext;
use cmx_api_core::{ApiResp, CmxAppState, Result};
use cmx_database_pg::get_default_pg_db_manager;
use cmx_mdm_store_pg as store;

/// 列激活映射（摘自 handlers/activation.rs 的真实模式）。
pub async fn mdm_activations_list(
    State(_s): State<CmxAppState>,
    CmxSvrContext(_ctx): CmxSvrContext,
    headers: HeaderMap,
    Query(q): Query<ActivationListQuery>,   // sourceDocType/crType/targetDict 可选过滤
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let mm = get_default_pg_db_manager();          // DB 走全局单例，不经 State 注入
    let db_id = resolve_db_id_from_headers(&headers).await; // 多数据源路由
    let list = store::list(mm, &db_id, None, None, None).await?;
    Ok(Json(ApiResp::ok(serde_json::json!(list))))
}
```

---

## Features 说明

| Feature | 默认 | 说明 |
|---------|------|------|
| `default` | ✅ | 仅内置 webhook 通道 |
| `channel-kafka` | ❌ | M5.3 MQ 通道开关：引入 rdkafka 客户端并编译 Kafka 通道骨架实现 |
| `channel-rocketmq` | ❌ | M5.3 MQ 通道开关：引入 rocketmq 客户端并编译 RocketMQ 通道骨架实现 |

---

## 常见问题

### Q1: 为什么 `cmx-api` 不依赖本 crate？

路由合并方向必须单向：`MdmModule` 实现 `cmx-api-core` 的 `ModuleRoutes`，由组装层
`cmx-platform-app` 调 `.merge(MdmModule.routes())`。若 cmx-api 反向依赖本 crate 会形成环。

### Q2: `/mdm/flow/callback` 为什么不走用户鉴权？

流程平台是机器调用方，无用户 JWT。安全性由 HMAC-SHA256 验签保证（`webhook_secret`
配置在 `[mdm.flow]` 段），**签名即凭证**，因此该端点注册在免用户鉴权路径上。

### Q3: 提交 CR 时「抢占」是什么？

`try_set_cr_status_pub(mm, db_id, None, cr_id, &["draft","rejected"], "approving")`
是一条条件 UPDATE：仅当当前状态在 from 集合内才迁移。双击/双端并发提交时只有一次
UPDATE 影响 1 行，其余拿到 0 行即返回 409「单据状态已变更」，无需行锁。
