# cmx-job-core

> 异步任务中心内核：作业领域模型 + 状态机 + `JobHandler`/`JobContext` 业务接触面 + `JobManager` 生命周期调度 + `JobEventHub` SSE 扇出——语义中立，零业务、零 DB 依赖。

[![Version](https://img.shields.io/badge/version-0.1.12-blue.svg)]()
[![Edition](https://img.shields.io/badge/rust--edition-2024-orange.svg)]()
[![Authors](https://img.shields.io/badge/authors-skylake%40pansoft.com-lightgrey.svg)]()

---

## 项目简介

`cmx-job-core` 为 CMX 平台的长时后端任务（报表计算/校验、凭证记账、销售汇总、常驻消息消费…）提供统一的作业生命周期：提交 → 排队 → 执行（协作式暂停/停止/重启）→ 终态 → 归档，并把进度以「快照 + 明细行」双层模型实时推送（SSE）。方案见 `docs/异步任务中心方案.html` 与 `docs/异步任务中心-常驻消费者作业方案.html`。

分层角色（三件套）：

- **本 crate（内核）**：模型 + 状态机 + trait + `JobManager` + `JobEventHub` + inventory 注册表；
- **`cmx-job-api`**：薄 axum handler + SSE 端点（`JobModule`）；
- **业务 crate（cmx-rpt 等）**：实现 `JobHandler` + `inventory::submit!` 一条注册（单向依赖本 crate，无环）；
- **`cmx-job-store-pg`**：实现本 crate 的 `JobStore` trait（core 不依赖 DB，破环方向与 ReportModule 一致）。

三个里程碑演进（同一套 API 向后兼容）：

- **M1 内存态**：`NullStore` 默认，进程重启即丢；暂停占 worker 槽；
- **M2 持久化**：注入 `Arc<dyn JobStore>` 后状态跃迁/去抖进度写穿，启动 `recover()` 崩溃恢复（幂等作业原 id 重跑、非幂等置失败）；schema 不可用自动降级内存态（warn，不阻塞 web-server 启动）；
- **M3 分布式**：`distributed=true` 时 submit 只写库，claim 循环经 `UPDATE...FOR UPDATE SKIP LOCKED` 抢占 pending 作业本地执行（多实例不重跑）；心跳 + reaper 回收失联属主；跨节点控制意图经 `control_intent` 列路由；`TerminalHook` 终态回调（失败告警接 GlobalEventBus——core 以回调解耦，不拖入 event_bus 重依赖）。

两种作业分型（`JobClass`）：**Batch**（默认，有界批处理，进度 = done/total 百分比）与 **Service**（常驻消费者，无自然终点，进度重新赋义为吞吐/速率，通常配 `singleton: true` 单例约束防重复消费，可按 `params.businessKey` 细分并存多个实例）。

---

## 与其他 crate 的关系

### 上游依赖（本 crate 依赖）

| 依赖 | 用途 |
|------|------|
| `tokio` | `OnceCell` 全局单例 / `Semaphore` 并发闸 / `watch` 控制通道 / `mpsc` 事件扇出 |
| `dashmap` | 作业表 `JobTable`（`DashMap<JobId, Job>`）+ 订阅路由表（并发安全，母版同 cmx-ai） |
| `async-trait` | `JobHandler::run` / `JobStore` 异步 trait |
| `inventory` | 编译期注册：业务 crate 一条 `inventory::submit!` 注册 handler（母版 = cmx-rpt-formula 函数注册） |
| `cmx-utils` | `next_pk_id` 主键铸号（52 位 JS 安全 bigint，进程重启不撞键） |
| `serde` / `serde_json` | 模型与事件载荷序列化 |
| `futures` / `tracing` | SSE 流基础（api 侧消费）/ 日志 |

### 下游使用者（谁依赖本 crate）

| 使用方 | 引用方式 | 实际用途 |
|--------|---------|---------|
| `cmx-job-api` | workspace 依赖 | HTTP 皮肤：handler 调 `manager()` 的 submit/pause/resume/cancel/restart/list/archive…，`hub()`/`summary_hub()` 订阅 SSE |
| `cmx-job-store-pg` | workspace 依赖 | 实现 `JobStore` trait（PG 持久化） |
| `cmx-platform-app` | workspace 依赖 | `config/jobs.rs` 的 `init_job_center()`：装配 JobConfig + PgJobStore + 失败告警 TerminalHook |
| `cmx-report`（跨仓库） | path 依赖 | `cmx-rpt-store-pg` 的 `rpt_job.rs` 实现 `JobHandler`（kind `"rpt.compute"`）注册报表计算作业 |

---

## 核心功能与特性

| 功能 | 入口 | 说明 |
|------|------|------|
| 作业模型与状态机 | `model.rs` | `JobStatus` 七态（Pending→Running→Paused/Cancelling→Cancelled/Completed/Failed），`is_terminal`/`is_active`/`as_str` |
| 进度双层表达 | `ProgressSnapshot` + `ProgressItem` | 快照（phase/done/total/ok/failed/message/eta_ms）+ 明细行（key 寻址 upsert）；`rev` 单调递增供前端去重抗乱序 |
| 业务处理器 | `JobHandler` trait | `kind()`/`capabilities()`/`plan()`/`run()` 四方法；能力声明 `JobCaps`（pausable/restart/idempotent/kind_class/singleton）驱动 UI 按钮与恢复策略 |
| 进度上报句柄 | `JobContext` | `checkpoint()`（协作检查点）/`set_phase`/`set_total`/`message`/`add_item`/`item_running·ok·fail`/`progress_inc`/`log·info·warn`，全部同步非阻塞，克隆廉价 |
| 生命周期调度 | `JobManager` | submit 入队 + `tokio::spawn` + `Semaphore` 并发闸（公平近似 FIFO）；结果映射终态；panic 被 catch_unwind 兜为 Failed |
| 事件扇出 | `JobEventHub` | 按 job_id 扇出 SSE（一作业多订阅者/多标签页）；无订阅者静默丢弃；`SUMMARY_CHANNEL`（虚拟 id=0）汇总频道广播列表页 `job` 事件 |
| 编译期注册 | `registry.rs` | `inventory::submit! { RegisteredJob { make: \|\| Box::new(MyJob) } }` 即完成注册，无运行时注册 API |
| 单例约束 | `JobCaps.singleton` | 活跃锁 key = `kind [:: businessKey]`；同 key 已有活跃实例时重复提交返回 409（JobError.code=409） |
| 崩溃恢复 | `JobManager::recover` | `load_active()` 读残留非终态：幂等 → 原 id 重新入队（requeue_existing）；非幂等 → 置 Failed（"进程重启中断，非幂等作业需人工重启"） |
| 分布式三循环 | `spawn_distributed_loops` | claim（1s，按并发闸空槽数量抢占）/ heartbeat+reaper（5s，`owner_timeout_ms` 判失联）/ control（1s，消费跨节点意图）；表驱动轮询（deadpool 无持久连接可挂 LISTEN/NOTIFY） |
| 持久化接缝 | `JobStore` trait | 18 个方法（写穿 + 查询 + M3 分布式）；`NullStore` 零成本默认 |
| 内置演示 | `DemoJob` / `DemoConsumerJob` | kind `job.demo`（N 步批处理，params: steps/stepMs/failAt/failWhole）与 `job.consumer`（常驻消费循环，优雅关闭 drain→Ok→Completed；每 17 条模拟一次死信）；供前端「任务中心」自检与端到端冒烟 |

---

## 模块结构

```text
cmx-job-core
├── src
│   ├── lib.rs        # 全局单例装配（init_job_subsystem 三入口 + manager()）+ 内置 demo 两个作业 + 7 个端到端测试（525 行）
│   ├── model.rs      # 领域模型与状态机：Job/JobStatus/ProgressSnapshot/ProgressItem/JobCaps/JobClass/Restart/SubmitRequest…（378 行）
│   ├── context.rs    # 业务接触面：JobHandler trait + JobContext 句柄 + Control/JobCancelled/LogLevel（207 行）
│   ├── manager.rs    # JobManager：提交/并发闸/控制路由/生命周期/恢复/分布式三循环 + JobConfig/ControlOutcome/TerminalHook（1033 行）
│   ├── event.rs      # JobEvent（8 种 SSE 帧构造器）+ JobEventHub（按 job_id 扇出）（209 行）
│   ├── runtime.rs    # 单作业运行态 JobRuntime：快照权威更新 + 控制通道 + 事件扇出桥 + JobTable/now_ms（195 行）
│   ├── registry.rs   # RegisteredJob + build_registry + registered_kinds（42 行）
│   └── store.rs      # JobStore 持久化 trait（18 方法）+ NullStore（129 行）
└── Cargo.toml
```

---

## 关键类型 / API

```rust
// ── lib.rs：全局单例 ──
pub async fn init_job_subsystem(cfg: JobConfig);                        // M1：内存态（NullStore）
pub async fn init_job_subsystem_with_store(cfg: JobConfig, store: Arc<dyn JobStore>);
pub async fn init_job_subsystem_full(cfg: JobConfig, store: Arc<dyn JobStore>,
    hook: Option<manager::TerminalHook>);                               // M3：+终态回调+分布式循环
pub fn manager() -> Option<&'static JobManager>;                        // 未初始化返回 None
// 幂等（OnceCell 首次生效）；启动序列 ensure_schema → 构建 → recover(单机)/spawn_distributed_loops(分布式)

// ── model.rs：领域模型 ──
pub type JobId = i64;
pub enum JobStatus { Pending, Running, Paused, Cancelling, Cancelled, Completed, Failed }
    // is_terminal()（终态可清理）/ is_active()（占 worker 槽）/ as_str()
pub struct ProgressSnapshot {
    pub phase: String, pub phase_index: u32, pub phase_total: u32,
    pub done: u64, pub total: u64,     // total=0 表示未知基数（percent() 恒 0）
    pub ok: u64, pub failed: u64, pub message: String, pub eta_ms: Option<u64>,
    pub items: Vec<ProgressItem>, pub rev: u64,
}   // impl: percent() -> u32（0–100，越界保护）
pub struct ProgressItem { pub key: String, pub label: String,
    pub state: ItemState, pub detail: String }   // ItemState: Queued/Running/Ok/Failed/Skipped
pub struct Job { id, kind, title, params: Value, status, progress,
    result: Option<Value>, error: Option<JobError>, priority: i16,
    origin: JobOrigin, org_id: Option<i64>, created_by: Option<i64>,
    created_at: i64, started_at/finished_at: Option<i64> }
pub enum JobOrigin { Frontend { user: Option<String> }, Backend { trigger: String } }
pub struct JobError { pub code: u16, pub message: String,
    pub violations: Vec<Value> }   // new(code, msg) / cancelled() -> 499"作业已被停止"
pub struct SubmitRequest { pub kind: String, pub params: Value,
    pub title: Option<String>, pub priority: Option<i16> }
pub enum Restart { None, Fresh, Resume }          // 当前统一按 Fresh 派生新作业
pub enum JobClass { Batch, Service }
pub struct JobCaps { pub pausable: bool, pub restart: Restart, pub idempotent: bool,
    pub kind_class: JobClass, pub singleton: bool }  // Default: true/Fresh/true/Batch/false
pub struct JobPlan { pub total: u64, pub title: Option<String> }

// ── context.rs：业务接触面 ──
pub enum Control { Run, Pause, Cancel }
pub struct JobCancelled;                     // From<JobCancelled> for JobError → 499
#[async_trait]
pub trait JobHandler: Send + Sync {
    fn kind(&self) -> &'static str;
    fn capabilities(&self) -> JobCaps { JobCaps::default() }
    fn plan(&self, _params: &Value) -> Result<JobPlan, JobError> { Ok(JobPlan::default()) }
    async fn run(&self, ctx: &JobContext, params: Value) -> Result<Value, JobError>;
}
#[derive(Clone)]
pub struct JobContext { /* Arc<JobRuntime>，克隆廉价 */ }
impl JobContext {
    pub fn job_id(&self) -> i64;
    pub async fn checkpoint(&self) -> Result<(), JobCancelled>;  // 协作检查点（见下）
    pub fn set_phase(&self, index: u32, total: u32, name: impl Into<String>);
    pub fn set_total(&self, total: u64);
    pub fn message(&self, text: impl Into<String>);
    pub fn add_item(&self, key: impl Into<String>, label: impl Into<String>);
    pub fn item_running(&self, key: &str, detail: impl Into<String>);
    pub fn item_ok(&self, key: &str, elapsed_ms: u64);
    pub fn item_fail(&self, key: &str, err: impl Display);
    pub fn progress_inc(&self, n: u64);       // 同时外推 eta_ms
    pub fn log(&self, level: LogLevel, text: impl Into<String>);
    pub fn info(&self, text: impl Into<String>);
    pub fn warn(&self, text: impl Into<String>);
}

// ── manager.rs：调度与控制 ──
pub struct JobConfig { pub max_concurrency: usize /*默认4*/, pub distributed: bool /*默认false*/,
    pub node_id: String /*默认"local"*/, pub owner_timeout_ms: i64 /*默认30_000*/ }
pub enum ControlOutcome { Accepted, NotFound, Rejected(String) }  // api 层映射 200/404/409
pub type TerminalHook = Arc<dyn Fn(&Job) + Send + Sync>;
pub const SUMMARY_CHANNEL: i64 = 0;
#[derive(Clone)]
pub struct JobManager;
impl JobManager {
    pub fn new(cfg: JobConfig) -> Self;                       // 内存态
    pub fn with_store(cfg: JobConfig, store: Arc<dyn JobStore>) -> Self;
    pub fn with_terminal_hook(self, hook: TerminalHook) -> Self;
    pub fn is_distributed(&self) -> bool;  pub fn node_id(&self) -> &str;
    pub fn hub(&self) -> Arc<JobEventHub>;                    // 单作业 SSE 订阅
    pub fn summary_hub(&self) -> Arc<JobEventHub>;            // 列表页汇总流（订阅 SUMMARY_CHANNEL）
    pub fn kinds(&self) -> Vec<&'static str>;
    pub fn caps_of(&self, kind: &str) -> Option<JobCaps>;
    pub fn kinds_meta(&self) -> Vec<(&'static str, JobCaps)>; // 前端区分 Batch/Service、单例提示
    pub async fn submit(&self, req: SubmitRequest, origin: JobOrigin) -> Result<JobId, JobError>;
    pub async fn pause(&self, id: JobId) -> ControlOutcome;
    pub async fn resume(&self, id: JobId) -> ControlOutcome;
    pub async fn cancel(&self, id: JobId) -> ControlOutcome;
    pub async fn restart(&self, id: JobId) -> Result<JobId, ControlOutcome>; // Fresh 派生新作业
    pub async fn get(&self, id: JobId) -> Option<Job>;        // 内存热态→store 兜底
    pub fn get_hot(&self, id: JobId) -> Option<Job>;          // 仅内存（不查库）
    pub fn snapshot_event(&self, id: JobId) -> Option<JobEvent>; // SSE 首帧
    pub async fn list(&self, kind: Option<&str>, status: Option<JobStatus>, limit: usize) -> Vec<Job>;
    pub async fn archive(&self, id: JobId) -> ControlOutcome;  // RU/HI：仅终态，转历史表
    pub async fn list_history(&self, kind: Option<&str>, status: Option<JobStatus>,
        offset: usize, limit: usize) -> Vec<Job>;
    pub async fn get_history(&self, id: JobId) -> Option<Job>;
    pub async fn count_history(&self, kind: Option<&str>, status: Option<JobStatus>) -> u64;
    pub async fn recover(&self);                               // 单机启动崩溃恢复
    pub fn spawn_distributed_loops(&self);                     // 分布式三循环
}

// ── event.rs：SSE 事件 ──
pub struct JobEvent { pub event_name: &'static str, pub payload: String }
impl JobEvent {  // 构造器
    pub fn new(event_name: &'static str, payload: impl Serialize) -> Self;
    // snapshot（首帧全量+状态）/ state（跃迁）/ progress（快照头部，percent 计算）
    // item（明细行 upsert）/ log / result / error / done（流终结）
}
pub struct JobEventHub;
impl JobEventHub {
    pub fn subscribe(&self, job_id: i64) -> UnboundedReceiver<JobEvent>;  // 可多次订阅
    pub fn broadcast(&self, job_id: i64, event: JobEvent);  // 无订阅者静默丢弃
    pub fn clear(&self, job_id: i64);  pub fn has_subscribers(&self, job_id: i64) -> bool;
}

// ── registry.rs / store.rs ──
pub struct RegisteredJob { pub make: fn() -> Box<dyn JobHandler> }
pub fn build_registry() -> HashMap<&'static str, fn() -> Box<dyn JobHandler>>;
pub fn registered_kinds() -> Vec<&'static str>;
pub trait JobStore: Send + Sync { /* 18 方法，见 store.rs；PG 实现见 cmx-job-store-pg */ }
pub struct NullStore;
```

---

## 使用示例

### 一、业务 crate 实现并注册一个作业（cmx-report / cmx-rpt-store-pg 的 rpt_job.rs 同款姿势）

```rust
use cmx_job_core::{JobCaps, JobClass, JobContext, JobError, JobHandler, JobPlan};

pub struct RptComputeJob;

#[async_trait::async_trait]
impl JobHandler for RptComputeJob {
    fn kind(&self) -> &'static str { "rpt.compute" }

    fn capabilities(&self) -> JobCaps {
        JobCaps { pausable: true, restart: cmx_job_core::Restart::Fresh,
                  idempotent: true, ..Default::default() }  // 批处理：默认 Batch 即可
    }

    fn plan(&self, params: &serde_json::Value) -> Result<JobPlan, JobError> {
        // 廉价预估：解析 params 算出进度条基数（报表张数）
        let n = params.get("reportCodes").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(1);
        Ok(JobPlan { total: n as u64, title: Some("报表计算".into()) })
    }

    async fn run(&self, ctx: &JobContext, params: serde_json::Value)
        -> Result<serde_json::Value, JobError>
    {
        ctx.set_phase(1, 2, "装载模板");
        for code in ["BS", "PL", "CF"] {
            ctx.checkpoint().await?;          // 协作检查点：暂停真挂起、停止优雅退出
            ctx.add_item(code, format!("{code} 表"));
            ctx.item_running(code, "计算中");
            // …真实计算…
            ctx.item_ok(code, 1200);
            ctx.progress_inc(1);
            ctx.info(format!("{code} 完成"));
        }
        Ok(serde_json::json!({ "ok": 3 }))
    }
}

// 一条注册（编译期收集，无需碰框架代码）
inventory::submit! { cmx_job_core::RegisteredJob { make: || Box::new(RptComputeJob) } }
```

### 二、web-server 启动装配（摘自 cmx-platform-app/src/config/jobs.rs）

```rust
pub async fn init_job_center() {
    let node_id = std::env::var("CMX_JOB_NODE_ID").ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| format!("node-{}", std::process::id()));
    let distributed = std::env::var("JOB_DISTRIBUTED")
        .map(|v| v != "false" && v != "0").unwrap_or(true);
    let job_cfg = cmx_job_core::JobConfig {
        max_concurrency: 4, distributed, node_id, owner_timeout_ms: 30_000,
    };
    // 终态回调：失败作业发 GlobalEventBus（job.failed），供告警消费者订阅
    let hook: cmx_job_core::manager::TerminalHook = std::sync::Arc::new(|job: &cmx_job_core::Job| {
        if job.status == cmx_job_core::JobStatus::Failed {
            let payload = serde_json::json!({
                "id": job.id.to_string(), "kind": job.kind,
                "title": job.title, "error": job.error,
            });
            tokio::spawn(async move {
                if cmx_traits::event_bus::GlobalEventBus::is_initialized() {
                    cmx_traits::event_bus::GlobalEventBus::get().publish("job.failed", payload).await;
                }
            });
        }
    });
    cmx_job_core::init_job_subsystem_full(
        job_cfg,
        std::sync::Arc::new(cmx_job_store_pg::PgJobStore::default_db()),
        Some(hook),
    ).await;
}
```

### 三、后端提交 + 控制 + 轮询终态（对齐 lib.rs 内置测试）

```rust
let mgr = cmx_job_core::manager().expect("任务中心未初始化");

// 提交（Frontend/Backend 共用入口；单例冲突会得到 code=409 的 JobError）
let id = mgr.submit(
    SubmitRequest {
        kind: "job.demo".into(),
        params: serde_json::json!({ "steps": 100, "stepMs": 20 }),
        title: None, priority: None,
    },
    JobOrigin::Backend { trigger: "timer".into() },
).await.unwrap();

tokio::time::sleep(std::time::Duration::from_millis(50)).await;
assert!(matches!(mgr.pause(id).await, ControlOutcome::Accepted));   // → Paused，进度冻结
assert!(matches!(mgr.resume(id).await, ControlOutcome::Accepted));  // → 零延迟唤醒
assert!(matches!(mgr.cancel(id).await, ControlOutcome::Accepted));  // → Cancelling → Cancelled

// 轮询直至终态（get_hot 只读内存热态，不查库）
loop {
    if let Some(j) = mgr.get_hot(id) {
        if j.status.is_terminal() { println!("终态: {:?} {}", j.status, j.progress.message); break; }
    }
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
}
```

### 四、订阅单作业 SSE 流（cmx-job-api handlers 的真实用法骨架）

```rust
let mgr = cmx_job_core::manager().unwrap();
// ① 先订阅（防止订阅建立前的广播丢失）
let mut rx = mgr.hub().subscribe(id);
// ② 补发 snapshot 首帧（Hub 不缓存快照）
if let Some(ev) = mgr.snapshot_event(id) { /* 直接作为 SSE 首帧写出 */ }
// ③ 之后循环 rx.recv().await 收 state/progress/item/log/result/error/done 增量，
//    转成 axum Event（event = ev.event_name, data = ev.payload）。
// 列表页则订阅汇总流：mgr.summary_hub().subscribe(cmx_job_core::SUMMARY_CHANNEL)
```

---

## 设计说明

- **协作式控制**：控制经每作业一路 `watch::Sender<Control>` 下发（watch 只留最新值，天然去抖+幂等）；handler 在长循环周期调 `checkpoint()`——`Pause` 时真挂起（`watch::changed().await`，零轮询零 CPU），`Cancel` 返回 `Err(JobCancelled)` 用 `?` 优雅退出。不埋 checkpoint 的作业仍能跑完，但不可暂停/停止（`JobCaps.pausable=false` 时 UI 隐藏按钮）。
- **Service 分型的优雅关闭**：常驻消费者在批间 checkpoint 处收到 Cancel → 先 drain 手头批 → 返回 `Ok(累计摘要)` → 落 **Completed**（而非 Cancelled）；`total=0` 使 `percent()` 恒 0，前端改渲染吞吐仪表（速率/累计/上批量）。
- **进度落库去抖**：进度是内存权威，写穿按 `PROGRESS_PERSIST_STEP`（每推进 10 条明细）+ 阶段切换/终态必落，避免高频 UPDATE 打爆库。
- **内存优先合并**：`list` 合并内存热态与 store 历史，按 id 去重时内存覆盖（较新权威），再按 `created_at DESC` 排序截断——进程重启后列表不空。
- **单例锁二次校验**：同 key 活跃锁命中后会校验该作业确实还活着（终态残留则放行覆盖），防异常路径下的锁泄漏。
