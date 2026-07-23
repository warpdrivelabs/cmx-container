//! [`JobManager`]：提交 / 排队 / 并发闸 / 控制路由 / 生命周期（方案 §7.3、§4）。
//!
//! M1 调度模型（spawn-per-job + 信号量并发闸）：
//!   - `submit` 建 Job(Pending) 入表，`tokio::spawn` 一个执行任务；
//!   - 任务先 `acquire` 一个信号量许可（= 排队/并发闸；tokio Semaphore 公平，近似 FIFO）；
//!   - 拿到许可后原子地 Pending→Running，构造 [`JobContext`]，跑 `handler.run`；
//!   - 结果映射终态：Ok→Completed / Err→Failed / 取消→Cancelled；panic 被 catch_unwind 兜为 Failed。
//! 控制经每作业一路 `watch::Sender<Control>` 下发，checkpoint 侧响应（协作式）。
//!
//! 内部全 `Arc` 共享，克隆廉价；单例由 `crate::init_job_subsystem` 持有。

use std::collections::HashMap;
use std::sync::Arc;

use dashmap::DashMap;
use futures::FutureExt;
use serde_json::Value;
use tokio::sync::{Semaphore, watch};

use crate::context::{Control, JobContext, JobHandler};
use crate::event::{JobEvent, JobEventHub};
use crate::model::{
    Job, JobError, JobId, JobOrigin, JobStatus, ProgressSnapshot, Restart, SubmitRequest,
};
use crate::registry::build_registry;
use crate::runtime::{JobRuntime, JobTable, now_ms};
use crate::store::{JobStore, NullStore};

/// 任务中心配置。
#[derive(Debug, Clone)]
pub struct JobConfig {
    /// 最大并发执行作业数（并发闸；超出的排队等待）。
    pub max_concurrency: usize,
    /// 分布式模式（M3）：true=多实例抢占池（submit 只写库，claim 循环驱动执行）；
    /// false=单机直跑（submit 直接 spawn，M1/M2 行为）。需配 PgJobStore 才有意义。
    pub distributed: bool,
    /// 本节点标识（分布式抢占的属主标记）。默认 "local"。
    pub node_id: String,
    /// 属主心跳超时（ms）：超过则 reaper 判失联、回收其作业。默认 30s。
    pub owner_timeout_ms: i64,
}

impl Default for JobConfig {
    fn default() -> Self {
        Self {
            max_concurrency: 4,
            distributed: false,
            node_id: "local".into(),
            owner_timeout_ms: 30_000,
        }
    }
}

/// 控制操作的结果（供 handler 层映射 HTTP 语义）。
#[derive(Debug)]
pub enum ControlOutcome {
    /// 操作已接受。
    Accepted,
    /// 作业不存在（→ 404）。
    NotFound,
    /// 当前状态不允许该操作（→ 409，附原因）。
    Rejected(String),
}

struct Inner {
    table: JobTable,
    hub: Arc<JobEventHub>,
    /// 汇总事件枢纽：作业级状态变化广播到「订阅全部」的前端（列表页实时刷新，方案 §6.1）。
    /// 复用 JobEventHub，固定用 SUMMARY_CHANNEL 这一虚拟 job_id 作为广播频道。
    summary_hub: Arc<JobEventHub>,
    registry: HashMap<&'static str, fn() -> Box<dyn JobHandler>>,
    /// 每个「活跃」作业一路控制通道（Running/Paused/Cancelling 期间存在）。
    controls: DashMap<JobId, watch::Sender<Control>>,
    sem: Arc<Semaphore>,
    /// 持久化后端（M2）；M1 默认 NullStore（no-op）。
    store: Arc<dyn JobStore>,
    /// 运行时配置（分布式模式 / node_id / 心跳超时等，M3）。
    cfg: JobConfig,
    /// 进度落库去抖：`{job_id → 上次落库的 done 计数}`，间隔 PROGRESS_PERSIST_STEP 才写。
    progress_persist_mark: DashMap<JobId, u64>,
    /// 日志 seq 递增：`{job_id → 下一条日志 seq}`。
    log_seq: DashMap<JobId, i64>,
    /// 常驻/单例作业的活跃锁：`{singleton_key → job_id}`。同 key 已有活跃实例时，
    /// 重复提交返回 409（常驻消费者作业方案 §6.1，母版 = cmx-ai session 活跃锁）。
    active_singletons: DashMap<String, JobId>,
    /// 终态回调（M3 失败告警）：作业进入终态时同步触发，由 web-server 注入（接 GlobalEventBus）。
    /// core 不依赖 event_bus（避免拖入 extism 等重依赖），以回调解耦。
    on_terminal: Option<TerminalHook>,
}

/// 终态回调类型：作业落终态时以 `&Job` 触发（失败告警 / 完成通知等）。
pub type TerminalHook = Arc<dyn Fn(&Job) + Send + Sync>;

/// 汇总频道的虚拟 job_id（列表页订阅它收所有作业的状态变化）。
pub const SUMMARY_CHANNEL: i64 = 0;

/// 进度落库去抖步长：每推进 N 条明细才写一次 progress（阶段切换/终态另行必落）。
const PROGRESS_PERSIST_STEP: u64 = 10;

/// 全局任务管理器（Arc 共享内部态，克隆廉价）。
#[derive(Clone)]
pub struct JobManager {
    inner: Arc<Inner>,
}

impl JobManager {
    /// 构造（内存态，NullStore）——M1 兼容入口。
    pub fn new(cfg: JobConfig) -> Self {
        Self::with_store(cfg, Arc::new(NullStore))
    }

    /// 构造并注入持久化后端（M2）。收集 `inventory` 注册的全部 handler。
    pub fn with_store(cfg: JobConfig, store: Arc<dyn JobStore>) -> Self {
        let registry = build_registry();
        let sem = Arc::new(Semaphore::new(cfg.max_concurrency.max(1)));
        Self {
            inner: Arc::new(Inner {
                table: Arc::new(DashMap::new()),
                hub: Arc::new(JobEventHub::new()),
                summary_hub: Arc::new(JobEventHub::new()),
                registry,
                controls: DashMap::new(),
                sem,
                store,
                cfg,
                progress_persist_mark: DashMap::new(),
                log_seq: DashMap::new(),
                active_singletons: DashMap::new(),
                on_terminal: None,
            }),
        }
    }

    /// 注入终态回调（M3 失败告警）：web-server 调此把终态作业转发到 GlobalEventBus。
    /// 须在 with_store 之后、spawn_distributed_loops 之前调用（构造期一次性）。
    pub fn with_terminal_hook(mut self, hook: TerminalHook) -> Self {
        // Arc<Inner> 独占时可 get_mut 改字段（构造链上唯一持有者）。
        if let Some(inner) = Arc::get_mut(&mut self.inner) {
            inner.on_terminal = Some(hook);
        } else {
            tracing::warn!("with_terminal_hook: Inner 已被共享，回调未注入");
        }
        self
    }

    /// 是否分布式模式。
    pub fn is_distributed(&self) -> bool {
        self.inner.cfg.distributed
    }

    /// 本节点标识。
    pub fn node_id(&self) -> &str {
        &self.inner.cfg.node_id
    }

    /// 事件枢纽（cmx-job-api 订阅单作业 SSE 用）。
    pub fn hub(&self) -> Arc<JobEventHub> {
        self.inner.hub.clone()
    }

    /// 汇总事件枢纽（cmx-job-api 订阅列表页 SSE 用；订阅 [`SUMMARY_CHANNEL`]）。
    pub fn summary_hub(&self) -> Arc<JobEventHub> {
        self.inner.summary_hub.clone()
    }

    /// 广播一条汇总事件（作业级状态变化 → 列表页）。
    fn emit_summary(&self, job: &Job) {
        let ev = JobEvent::new(
            "job",
            serde_json::json!({
                "id": job.id.to_string(),
                "kind": job.kind,
                "title": job.title,
                "status": job.status.as_str(),
                "percent": job.progress.percent(),
                "done": job.progress.done,
                "total": job.progress.total,
                "ok": job.progress.ok,
                "failed": job.progress.failed,
                "message": job.progress.message,
                "createdAt": job.created_at,
                "finishedAt": job.finished_at,
            }),
        );
        self.inner.summary_hub.broadcast(SUMMARY_CHANNEL, ev);
    }

    /// 下一条日志 seq（预留：M2 日志走 SSE，DB 日志表留待 M3 消费）。
    #[allow(dead_code)]
    fn next_log_seq(&self, id: JobId) -> i64 {
        let mut e = self.inner.log_seq.entry(id).or_insert(0);
        *e += 1;
        *e
    }

    /// 已注册的作业种类。
    pub fn kinds(&self) -> Vec<&'static str> {
        let mut v: Vec<_> = self.inner.registry.keys().copied().collect();
        v.sort_unstable();
        v
    }

    /// 某作业种类的能力声明（用于前端识别 Service 类作业、渲染吞吐视图 / 单例约束提示）。
    pub fn caps_of(&self, kind: &str) -> Option<crate::model::JobCaps> {
        self.inner.registry.get(kind).map(|make| make().capabilities())
    }

    /// 已注册种类 + 元数据（class / singleton / pausable）。前端据此区分批处理与常驻消费者。
    pub fn kinds_meta(&self) -> Vec<(&'static str, crate::model::JobCaps)> {
        let mut v: Vec<_> = self
            .inner
            .registry
            .iter()
            .map(|(k, make)| (*k, make().capabilities()))
            .collect();
        v.sort_by_key(|(k, _)| *k);
        v
    }

    // ───────────────────────── 提交 ─────────────────────────

    /// 提交一个作业。前端 POST /api/jobs 与后端自发起共用此入口（方案 §11）。
    ///
    /// 返回新作业 id。kind 未注册 → `Err`（不建作业）。
    pub async fn submit(&self, req: SubmitRequest, origin: JobOrigin) -> Result<JobId, JobError> {
        let make = self
            .inner
            .registry
            .get(req.kind.as_str())
            .copied()
            .ok_or_else(|| {
                JobError::new(400, format!("未注册的作业种类: {}", req.kind))
            })?;
        let handler = make();
        let caps = handler.capabilities();

        // 单例约束（常驻消费者作业方案 §6.1）：同 singleton_key 已有活跃实例 → 拒绝（409 语义）。
        // key = kind [+ params.businessKey]，允许同种类按业务键并存多个单例消费者。
        let singleton_key = if caps.singleton {
            Some(singleton_key_of(&req.kind, &req.params))
        } else {
            None
        };
        if let Some(key) = &singleton_key {
            use dashmap::mapref::entry::Entry;
            match self.inner.active_singletons.entry(key.clone()) {
                Entry::Occupied(e) => {
                    let existing = *e.get();
                    // 二次校验存在的作业确实还活着；若已是终态残留（异常）则放行覆盖。
                    let alive = self
                        .inner
                        .table
                        .get(&existing)
                        .map(|j| !j.status.is_terminal())
                        .unwrap_or(false);
                    if alive {
                        return Err(JobError::new(
                            409,
                            format!("该作业已有活跃实例（#{existing}），单例约束拒绝重复启动"),
                        ));
                    }
                }
                Entry::Vacant(_) => {}
            }
        }

        // 提交期预估：total（进度条基数）+ 标题。plan 失败即拒绝提交（参数非法早暴露）。
        let plan = handler.plan(&req.params)?;
        let title = req
            .title
            .filter(|s| !s.trim().is_empty())
            .or(plan.title)
            .unwrap_or_else(|| format!("{} #{}", req.kind, "?"));

        let id = cmx_utils::next_pk_id();
        let now = now_ms();
        let job = Job {
            id,
            kind: req.kind.clone(),
            title: if title.ends_with("#?") {
                format!("{} #{}", req.kind, id)
            } else {
                title
            },
            params: req.params.clone(),
            status: JobStatus::Pending,
            progress: ProgressSnapshot {
                total: plan.total,
                message: "已入队，等待执行".into(),
                ..Default::default()
            },
            result: None,
            error: None,
            priority: req.priority.unwrap_or(0),
            origin,
            org_id: None,
            created_by: None,
            created_at: now,
            started_at: None,
            finished_at: None,
        };
        self.inner.table.insert(id, job.clone());
        // 占用单例锁（提交成功即登记；finish 时释放）。
        if let Some(key) = &singleton_key {
            self.inner.active_singletons.insert(key.clone(), id);
        }
        tracing::info!(job_id = id, kind = %req.kind, distributed = self.inner.cfg.distributed, "作业已提交入队");
        self.emit_summary(&job);

        // 预置控制通道（Pending 期就存在，便于「排队中取消」直达）。
        let (tx, _rx) = watch::channel(Control::Run);
        self.inner.controls.insert(id, tx);

        if self.inner.cfg.distributed {
            // 分布式模式：只写库（await 保证行落库可被任意节点抢占），不本地 spawn。
            // 执行由各节点的 claim 循环驱动——本节点或它节点领到后才真正跑。
            self.inner.store.insert(&job).await;
            // 内存热态标记为「待抢占」——从本节点内存表移除，避免 get 误报本地运行。
            // 保留一份轻量占位？直接移除：list/get 会回落 DB。
            self.inner.table.remove(&id);
        } else {
            // 单机模式（M1/M2）：直接 spawn 执行。insert 在 run_job 起始 await。
            let mgr = self.clone();
            let params = req.params;
            tokio::spawn(async move {
                mgr.run_job(id, make, params).await;
            });
        }

        Ok(id)
    }

    /// 单作业执行主体（单机模式 spawn 内运行：insert → 并发闸 → try_start → 执行）。
    async fn run_job(&self, id: JobId, make: fn() -> Box<dyn JobHandler>, params: Value) {
        // 先落库建行（await，保证行存在后再有任何 update_status/progress，杜绝乱序覆盖）。
        if let Some(job) = self.inner.table.get(&id).map(|j| j.clone()) {
            self.inner.store.insert(&job).await;
        }
        // 并发闸：拿许可即「出队开始」。许可持有至作业结束（暂停也占槽，方案 §4.4）。
        let _permit = match self.inner.sem.clone().acquire_owned().await {
            Ok(p) => p,
            Err(_) => return, // Semaphore 关闭（进程退出），静默收场。
        };

        // 拿到许可后：若排队期间已被取消，直接收场（不进 Running）。
        if !self.try_start(id) {
            // 非 Pending（多为排队中被 cancel 置了终态）——补发 done 收尾。
            self.inner.hub.broadcast(id, JobEvent::done());
            self.inner.controls.remove(&id);
            return;
        }
        self.execute_core(id, make, params).await;
    }

    /// 执行内核（单机 try_start 后 / 分布式 claim 后共用）：构造 context → 跑 handler → 落终态。
    ///
    /// 前置：作业已在内存表且状态为 running（try_start 或 claim 已置），控制通道已建。
    async fn execute_core(&self, id: JobId, make: fn() -> Box<dyn JobHandler>, params: Value) {
        // 构造运行态 + context。控制通道复用提交期预置的那路（Sender 在 controls 表）。
        // 从 Sender 派生一个持久 Receiver 交给 JobRuntime——只要它存活，通道就不关闭，
        // pause/cancel 的 send 在 handler sleep 期间也能成功送达。
        let control_tx = match self.inner.controls.get(&id).map(|r| r.clone()) {
            Some(tx) => tx,
            None => {
                let (tx, _) = watch::channel(Control::Run);
                self.inner.controls.insert(id, tx.clone());
                tx
            }
        };
        let control_rx = control_tx.subscribe();
        let rt = Arc::new(JobRuntime::new(
            id,
            self.inner.table.clone(),
            self.inner.hub.clone(),
            control_rx,
        ));
        let ctx = JobContext::new(rt);
        let handler = make();

        // 进度采样器：handler 跑动期间每 1s 采一次快照，去抖落库 + 汇总广播（不侵入 JobRuntime）。
        // 分布式模式下顺带刷心跳（证明本节点仍是活属主）。
        let sampler = {
            let mgr = self.clone();
            let (stop_tx, mut stop_rx) = watch::channel(false);
            let handle = tokio::spawn(async move {
                let mut tick = tokio::time::interval(std::time::Duration::from_secs(1));
                loop {
                    tokio::select! {
                        _ = tick.tick() => mgr.maybe_persist_progress(id),
                        _ = stop_rx.changed() => break,
                    }
                }
            });
            (stop_tx, handle)
        };

        // 执行（catch_unwind：handler panic 兜为 Failed，不留死任务）。
        let fut = std::panic::AssertUnwindSafe(handler.run(&ctx, params)).catch_unwind();
        let outcome = fut.await;

        // 停采样器。
        let _ = sampler.0.send(true);
        sampler.1.abort();

        // 结束意图判定：控制是否为 Cancel（决定 Err 归 Cancelled 还是 Failed）。
        let cancelled = matches!(*control_tx.borrow(), Control::Cancel);

        match outcome {
            Ok(Ok(result)) => self.finish(id, JobStatus::Completed, Some(result), None),
            Ok(Err(e)) => {
                if cancelled || e.code == 499 {
                    self.finish(id, JobStatus::Cancelled, None, Some(JobError::cancelled()));
                } else {
                    self.finish(id, JobStatus::Failed, None, Some(e));
                }
            }
            Err(panic) => {
                let msg = panic
                    .downcast_ref::<&str>()
                    .map(|s| s.to_string())
                    .or_else(|| panic.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "handler panic".into());
                self.finish(
                    id,
                    JobStatus::Failed,
                    None,
                    Some(JobError::new(500, format!("作业执行 panic: {msg}"))),
                );
            }
        }
        self.inner.controls.remove(&id);
    }

    // ───────────────────────── 状态跃迁 ─────────────────────────

    /// 原子 Pending→Running（并发闸放行时）。返回 false 表示已非 Pending（多为已取消）。
    fn try_start(&self, id: JobId) -> bool {
        let snap = {
            let mut job = match self.inner.table.get_mut(&id) {
                Some(j) => j,
                None => return false,
            };
            if job.status != JobStatus::Pending {
                return false;
            }
            job.status = JobStatus::Running;
            job.started_at = Some(now_ms());
            job.progress.message = "执行中".into();
            job.progress.rev += 1;
            job.clone()
        };
        self.inner
            .hub
            .broadcast(id, JobEvent::state(JobStatus::Running, snap.progress.rev));
        self.emit_summary(&snap);
        self.persist_status(snap);
        true
    }

    /// 落终态（幂等：已终态则忽略）：写状态/结果/错误 + 广播 result|error + done。
    fn finish(
        &self,
        id: JobId,
        status: JobStatus,
        result: Option<Value>,
        error: Option<JobError>,
    ) {
        let (snap, res_ev, err_ev) = {
            let mut job = match self.inner.table.get_mut(&id) {
                Some(j) => j,
                None => return,
            };
            if job.status.is_terminal() {
                return;
            }
            job.status = status;
            job.finished_at = Some(now_ms());
            job.progress.rev += 1;
            job.progress.eta_ms = None;
            job.progress.message = match status {
                JobStatus::Completed => "已完成".into(),
                JobStatus::Failed => "已失败".into(),
                JobStatus::Cancelled => "已停止".into(),
                _ => job.progress.message.clone(),
            };
            job.result = result.clone();
            job.error = error.clone();
            (job.clone(), result, error)
        };
        self.inner
            .hub
            .broadcast(id, JobEvent::state(status, snap.progress.rev));
        if let Some(r) = res_ev {
            self.inner.hub.broadcast(id, JobEvent::result(&r));
        }
        if let Some(e) = err_ev {
            self.inner.hub.broadcast(id, JobEvent::error(&e));
        }
        self.inner.hub.broadcast(id, JobEvent::done());
        self.emit_summary(&snap);
        self.persist_finish(snap.clone());
        self.inner.progress_persist_mark.remove(&id);
        self.inner.log_seq.remove(&id);
        // 释放单例锁（若本作业持有）：移除所有指向本 id 的 singleton 条目，放行下次启动。
        self.inner.active_singletons.retain(|_, v| *v != id);
        // 终态回调（失败告警等）：同步触发，回调内部自行 spawn 异步发布。
        if let Some(hook) = &self.inner.on_terminal {
            hook(&snap);
        }
        tracing::info!(job_id = id, status = status.as_str(), "作业终结");
    }

    // ───────────────────────── 持久化写穿（fire-and-forget，不阻塞）─────────────────────────

    /// 状态跃迁落库（必落）。
    fn persist_status(&self, job: Job) {
        let store = self.inner.store.clone();
        tokio::spawn(async move { store.update_status(&job).await });
    }

    /// 终态落库（含 result/error/finished_at）。
    fn persist_finish(&self, job: Job) {
        let store = self.inner.store.clone();
        tokio::spawn(async move { store.finish(&job).await });
    }

    /// 进度落库（采样器每 1s 调）：状态变化必落（跨节点可见 paused/cancelling），
    /// 进度按 PROGRESS_PERSIST_STEP 去抖落。用 update_status（写 status+progress）统一落，
    /// 保证分布式下他节点轮询能看到属主的暂停/恢复态。
    fn maybe_persist_progress(&self, id: JobId) {
        let job = match self.inner.table.get(&id) {
            Some(j) => j.clone(),
            None => return,
        };
        let last_done = self
            .inner
            .progress_persist_mark
            .get(&id)
            .map(|r| *r)
            .unwrap_or(0);
        // 状态变化检测：用 log_seq 表复用存不划算，直接比对 DB 无从得知——改为每次采样都
        // 落 status（幂等 UPDATE，1s 一次开销可忽略），既覆盖进度去抖又覆盖状态跃迁。
        let progressed = job.progress.done >= last_done + PROGRESS_PERSIST_STEP;
        let active_paused = matches!(job.status, JobStatus::Paused | JobStatus::Cancelling);
        if progressed || active_paused {
            if progressed {
                self.inner.progress_persist_mark.insert(id, job.progress.done);
            }
            self.emit_summary(&job);
            let store = self.inner.store.clone();
            tokio::spawn(async move { store.update_status(&job).await });
        }
    }

    // ───────────────────────── 控制（方案 §4.3；M3 跨节点路由）─────────────────────────

    /// 暂停。本节点属主→直接控制；他节点属主→写 DB control_intent（其 control 循环消费）。
    pub async fn pause(&self, id: JobId) -> ControlOutcome {
        // 本地热态优先（本节点在跑）。
        if let Some(s) = self.current_status(id) {
            return match s {
                JobStatus::Running | JobStatus::Pending => {
                    self.send_control(id, Control::Pause);
                    ControlOutcome::Accepted
                }
                _ => ControlOutcome::Rejected(format!("{} 状态不可暂停", s.as_str())),
            };
        }
        // 非本地：查 DB，若为他节点活跃作业 → 写意图跨节点路由。
        self.route_remote_control(id, "pause", "暂停").await
    }

    /// 恢复。同 pause 的跨节点路由。
    pub async fn resume(&self, id: JobId) -> ControlOutcome {
        if let Some(s) = self.current_status(id) {
            return match s {
                JobStatus::Paused | JobStatus::Pending => {
                    self.send_control(id, Control::Run);
                    self.local_resume_state(id);
                    ControlOutcome::Accepted
                }
                _ => ControlOutcome::Rejected(format!("{} 状态不可恢复", s.as_str())),
            };
        }
        self.route_remote_control(id, "resume", "恢复").await
    }

    /// 停止。本地属主直接停；他节点属主写意图；Pending（未被任何节点领）直接终结。
    pub async fn cancel(&self, id: JobId) -> ControlOutcome {
        if let Some(status) = self.current_status(id) {
            return match status {
                JobStatus::Pending => {
                    self.send_control(id, Control::Cancel);
                    self.finish(id, JobStatus::Cancelled, None, Some(JobError::cancelled()));
                    ControlOutcome::Accepted
                }
                JobStatus::Running | JobStatus::Paused => {
                    self.mark_cancelling(id);
                    self.send_control(id, Control::Cancel);
                    ControlOutcome::Accepted
                }
                s if s.is_terminal() => {
                    ControlOutcome::Rejected(format!("{} 已终结，无法停止", s.as_str()))
                }
                s => ControlOutcome::Rejected(format!("{} 状态不可停止", s.as_str())),
            };
        }
        self.route_remote_control(id, "cancel", "停止").await
    }

    /// 跨节点控制路由：查 DB 判定作业存在性/活跃性，活跃→写 control_intent（属主节点消费）。
    async fn route_remote_control(&self, id: JobId, intent: &str, verb: &str) -> ControlOutcome {
        match self.inner.store.get(id).await {
            None => ControlOutcome::NotFound,
            Some(job) if job.status.is_active() || job.status == JobStatus::Pending => {
                self.inner.store.set_control_intent(id, intent).await;
                tracing::info!(job_id = id, intent, "跨节点控制意图已写库，等属主节点消费");
                ControlOutcome::Accepted
            }
            Some(job) => ControlOutcome::Rejected(format!("{} 状态不可{}", job.status.as_str(), verb)),
        }
    }

    /// 重启：仅 Failed/Cancelled 终态；Fresh 模式派生新作业（原作业保留作审计，方案 §4.5）。
    pub async fn restart(&self, id: JobId) -> Result<JobId, ControlOutcome> {
        // 终态作业可能只在 DB（分布式他节点跑完 / 本节点重启后）——内存未命中回落 DB。
        let job = match self.inner.table.get(&id).map(|j| j.clone()) {
            Some(j) => j,
            None => match self.inner.store.get(id).await {
                Some(j) => j,
                None => return Err(ControlOutcome::NotFound),
            },
        };
        if !job.status.is_terminal() || job.status == JobStatus::Completed {
            return Err(ControlOutcome::Rejected(
                "仅失败/已停止的作业可重启".into(),
            ));
        }
        // 查 handler 能力：不支持重启则拒绝。
        let caps = self
            .inner
            .registry
            .get(job.kind.as_str())
            .map(|make| make().capabilities());
        match caps.map(|c| c.restart) {
            Some(Restart::Fresh) | Some(Restart::Resume) => {
                // M1/M2/M3：统一按 Fresh 用原 params 派生新作业（Resume 断点续跑留待后续）。
                let origin = job.origin.clone();
                self.submit(
                    SubmitRequest {
                        kind: job.kind.clone(),
                        params: job.params.clone(),
                        title: Some(job.title.clone()),
                        priority: Some(job.priority),
                    },
                    origin,
                )
                .await
                .map_err(|e| ControlOutcome::Rejected(e.message))
            }
            _ => Err(ControlOutcome::Rejected("该作业种类不支持重启".into())),
        }
    }

    fn mark_cancelling(&self, id: JobId) {
        let snap = {
            let mut job = match self.inner.table.get_mut(&id) {
                Some(j) => j,
                None => return,
            };
            if job.status.is_terminal() || job.status == JobStatus::Cancelling {
                return;
            }
            job.status = JobStatus::Cancelling;
            job.progress.message = "正在停止…".into();
            job.progress.rev += 1;
            job.clone()
        };
        self.inner
            .hub
            .broadcast(id, JobEvent::state(JobStatus::Cancelling, snap.progress.rev));
        self.emit_summary(&snap);
        self.persist_status(snap);
    }

    fn send_control(&self, id: JobId, c: Control) {
        if let Some(tx) = self.inner.controls.get(&id) {
            tx.send_replace(c);
            tracing::debug!(job_id = id, intent = ?c, "send_control 已下发");
        } else {
            tracing::warn!(job_id = id, intent = ?c, "send_control 找不到控制通道（作业非本节点在跑）");
        }
    }

    // ───────────────────────── 查询 ─────────────────────────

    fn current_status(&self, id: JobId) -> Option<JobStatus> {
        self.inner.table.get(&id).map(|j| j.status)
    }

    /// 取单作业（先内存热态，未命中回落持久化 store——重启后终态作业只在库里）。
    pub async fn get(&self, id: JobId) -> Option<Job> {
        if let Some(j) = self.inner.table.get(&id).map(|j| j.clone()) {
            return Some(j);
        }
        self.inner.store.get(id).await
    }

    /// 同步取内存热态单作业（SSE snapshot 首帧用；不查库）。
    pub fn get_hot(&self, id: JobId) -> Option<Job> {
        self.inner.table.get(&id).map(|j| j.clone())
    }

    /// 构造 SSE `snapshot` 首帧（订阅建立时补发，方案 §5.2）。
    pub fn snapshot_event(&self, id: JobId) -> Option<JobEvent> {
        self.inner
            .table
            .get(&id)
            .map(|j| JobEvent::snapshot(j.status, &j.progress))
    }

    /// 列表（按 kind/status 过滤，created_at 倒序，limit 截断）。
    ///
    /// 合并内存热态 + 持久化历史：热态作业（运行中/刚完成）以内存为准，
    /// 更早的历史从 store 读，按 id 去重（内存优先），再排序截断。
    pub async fn list(
        &self,
        kind: Option<&str>,
        status: Option<JobStatus>,
        limit: usize,
    ) -> Vec<Job> {
        use std::collections::HashMap as Map;
        let mut by_id: Map<JobId, Job> = Map::new();
        // 先放持久化历史（较旧/较全）。
        for j in self.inner.store.list(kind, status, limit.max(200)).await {
            by_id.insert(j.id, j);
        }
        // 内存热态覆盖（较新，权威）。
        for entry in self.inner.table.iter() {
            let j = entry.value();
            if kind.map(|k| j.kind == k).unwrap_or(true)
                && status.map(|s| j.status == s).unwrap_or(true)
            {
                by_id.insert(j.id, j.clone());
            }
        }
        let mut v: Vec<Job> = by_id.into_values().collect();
        v.sort_by(|a, b| b.created_at.cmp(&a.created_at).then(b.id.cmp(&a.id)));
        v.truncate(limit);
        v
    }

    /// 归档作业（原「删除」，语义调整为 RU/HI 分离，仅终态）：从活跃表转移到历史表，
    /// 清内存 + 订阅 + 控制通道。非真删——数据保留在历史表供查询/审计。
    pub async fn archive(&self, id: JobId) -> ControlOutcome {
        // 状态先看内存，内存无则查库（重启后终态作业只在库）。
        let status = match self.inner.table.get(&id).map(|j| j.status) {
            Some(s) => Some(s),
            None => self.inner.store.get(id).await.map(|j| j.status),
        };
        match status {
            None => ControlOutcome::NotFound,
            Some(s) if s.is_terminal() => {
                self.inner.store.archive(id).await;
                self.inner.table.remove(&id);
                self.inner.controls.remove(&id);
                self.inner.hub.clear(id);
                ControlOutcome::Accepted
            }
            Some(s) => ControlOutcome::Rejected(format!("{} 未终结，不可归档", s.as_str())),
        }
    }

    /// 历史作业列表（cmx_job_hi，按 kind/status 过滤，archived_at 倒序，分页 offset/limit）。
    pub async fn list_history(
        &self,
        kind: Option<&str>,
        status: Option<JobStatus>,
        offset: usize,
        limit: usize,
    ) -> Vec<Job> {
        self.inner.store.list_history(kind, status, offset, limit).await
    }

    /// 单条历史作业。
    pub async fn get_history(&self, id: JobId) -> Option<Job> {
        self.inner.store.get_history(id).await
    }

    /// 历史作业总数（与 list_history 同过滤）。
    pub async fn count_history(&self, kind: Option<&str>, status: Option<JobStatus>) -> u64 {
        self.inner.store.count_history(kind, status).await
    }

    // ───────────────────────── 崩溃恢复（方案 §8.3）─────────────────────────

    /// 启动期崩溃恢复：读出库中残留非终态作业，按 handler 幂等能力裁决——
    ///   - pending / 幂等作业 → 重新 submit（用原 params 派生执行，DB 行更新为新执行）；
    ///   - 非幂等作业 → 置 failed（重启中断，需人工处理）。
    ///
    /// 幂等作业按「派生新执行」而非「原地复活」——因为原作业的内存运行态已随进程消失，
    /// 无法续跑（M2 不做断点续跑）；派生保持 id 稳定性由 restart=Fresh 语义覆盖。
    /// 简化：直接原 id 重跑（保留 id，DB 行状态回到 running）。
    pub async fn recover(&self) {
        let orphans = self.inner.store.load_active().await;
        if orphans.is_empty() {
            return;
        }
        tracing::info!(count = orphans.len(), "崩溃恢复：发现残留非终态作业");
        for job in orphans {
            let idempotent = self
                .inner
                .registry
                .get(job.kind.as_str())
                .map(|make| make().capabilities().idempotent)
                .unwrap_or(false);
            if idempotent {
                // 幂等：原 id 重新入队执行（复用 submit 的调度，但保留原 id）。
                self.requeue_existing(job).await;
            } else {
                // 非幂等：置失败落库。
                let mut failed = job.clone();
                failed.status = JobStatus::Failed;
                failed.finished_at = Some(now_ms());
                failed.error = Some(JobError::new(500, "进程重启中断，非幂等作业需人工重启"));
                failed.progress.message = "进程重启中断".into();
                self.inner.store.finish(&failed).await;
                tracing::warn!(job_id = failed.id, kind = %failed.kind, "非幂等孤儿作业置失败");
            }
        }
    }

    /// 恢复一个幂等孤儿作业：原 id 重新入表 + 起执行任务。
    async fn requeue_existing(&self, job: Job) {
        let make = match self.inner.registry.get(job.kind.as_str()).copied() {
            Some(m) => m,
            None => return,
        };
        let id = job.id;
        let params = job.params.clone();
        let mut fresh = job;
        fresh.status = JobStatus::Pending;
        fresh.started_at = None;
        fresh.finished_at = None;
        fresh.result = None;
        fresh.error = None;
        fresh.progress = ProgressSnapshot {
            total: fresh.progress.total,
            message: "重启恢复，重新入队".into(),
            ..Default::default()
        };
        self.inner.table.insert(id, fresh.clone());
        self.inner.store.update_status(&fresh).await;
        let (tx, _rx) = watch::channel(Control::Run);
        self.inner.controls.insert(id, tx);
        let mgr = self.clone();
        tokio::spawn(async move {
            mgr.run_job(id, make, params).await;
        });
        tracing::info!(job_id = id, kind = %fresh.kind, "幂等孤儿作业已重新入队");
    }

    // ───────────────────────── M3 分布式（表驱动抢占池）─────────────────────────

    /// 启动分布式后台循环（仅 distributed 模式调用）：
    ///   - claim 循环：周期 `UPDATE...SKIP LOCKED` 抢占 pending 作业本地执行（多实例不重跑）；
    ///   - 心跳 + reaper 循环：刷本节点属主心跳 + 回收失联节点的孤儿作业；
    ///   - 控制意图循环：消费本节点属主作业的跨节点 pause/resume/cancel 意图。
    ///
    /// 表驱动轮询（非 LISTEN/NOTIFY）：deadpool 连接池无持久连接可挂 LISTEN，故沿用
    /// cmx-flow 定时器 poller 的周期扫库母版。轮询间隔取 1s，兼顾时延与库压。
    pub fn spawn_distributed_loops(&self) {
        if !self.inner.cfg.distributed {
            return;
        }
        let node = self.inner.cfg.node_id.clone();

        // ① claim 循环：抢占 pending 作业。
        {
            let mgr = self.clone();
            let node = node.clone();
            tokio::spawn(async move {
                let mut tick = tokio::time::interval(std::time::Duration::from_secs(1));
                loop {
                    tick.tick().await;
                    // 抢占空槽数量的作业（不超过并发闸剩余许可）。
                    let free = mgr.inner.sem.available_permits();
                    if free == 0 {
                        continue;
                    }
                    let claimed = mgr
                        .inner
                        .store
                        .claim_pending(&node, free, now_ms())
                        .await;
                    for job in claimed {
                        mgr.run_claimed(job);
                    }
                }
            });
        }

        // ② 心跳 + reaper 循环。
        {
            let mgr = self.clone();
            let node = node.clone();
            let timeout = self.inner.cfg.owner_timeout_ms;
            tokio::spawn(async move {
                let mut tick = tokio::time::interval(std::time::Duration::from_secs(5));
                loop {
                    tick.tick().await;
                    // 刷本节点属主作业心跳。
                    let owned: Vec<i64> = mgr
                        .inner
                        .table
                        .iter()
                        .filter(|e| e.value().status.is_active())
                        .map(|e| *e.key())
                        .collect();
                    if !owned.is_empty() {
                        mgr.inner.store.heartbeat(&node, &owned, now_ms()).await;
                    }
                    // reaper：回收失联节点的孤儿作业（置回 pending 供重领）。
                    let reaped = mgr.inner.store.reap_dead_owners(timeout, now_ms()).await;
                    if !reaped.is_empty() {
                        tracing::warn!(count = reaped.len(), "reaper 回收失联节点孤儿作业→pending");
                    }
                }
            });
        }

        // ③ 控制意图循环：消费跨节点 pause/resume/cancel。
        {
            let mgr = self.clone();
            let node = node.clone();
            tokio::spawn(async move {
                let mut tick = tokio::time::interval(std::time::Duration::from_secs(1));
                loop {
                    tick.tick().await;
                    let intents = mgr.inner.store.take_control_intents(&node).await;
                    for (id, intent) in intents {
                        tracing::info!(job_id = id, %intent, node = %node, "消费跨节点控制意图");
                        // 仅对本节点内存里实际在跑的作业生效（属主）。
                        if !mgr.inner.table.contains_key(&id) {
                            tracing::warn!(job_id = id, "控制意图：作业不在本节点内存，跳过");
                            continue;
                        }
                        match intent.as_str() {
                            "pause" => mgr.send_control(id, Control::Pause),
                            "resume" => {
                                mgr.send_control(id, Control::Run);
                                mgr.local_resume_state(id);
                            }
                            "cancel" => {
                                mgr.mark_cancelling(id);
                                mgr.send_control(id, Control::Cancel);
                            }
                            _ => {}
                        }
                    }
                }
            });
        }

        tracing::info!(node = %node, "任务中心分布式循环已启动（claim / heartbeat+reaper / control）");
    }

    /// 执行一个已被本节点 claim 的作业（DB 已置 running+node_id，放进内存表 → execute_core）。
    fn run_claimed(&self, job: Job) {
        let id = job.id;
        let make = match self.inner.registry.get(job.kind.as_str()).copied() {
            Some(m) => m,
            None => {
                tracing::warn!(job_id = id, kind = %job.kind, "claim 到未注册种类，跳过");
                return;
            }
        };
        let params = job.params.clone();
        // 放进内存表（execute_core / JobRuntime 就地更新其快照）。
        self.inner.table.insert(id, job.clone());
        // 控制通道。
        let (tx, _rx) = watch::channel(Control::Run);
        self.inner.controls.insert(id, tx);
        // 广播 running 状态 + 汇总（本节点订阅者立即可见）。
        self.inner
            .hub
            .broadcast(id, JobEvent::state(JobStatus::Running, job.progress.rev));
        self.emit_summary(&job);
        tracing::info!(job_id = id, kind = %job.kind, node = %self.inner.cfg.node_id, "claim 作业，本节点执行");
        // 持许可执行（占并发槽直至结束）。
        let mgr = self.clone();
        tokio::spawn(async move {
            let _permit = match mgr.inner.sem.clone().acquire_owned().await {
                Ok(p) => p,
                Err(_) => return,
            };
            mgr.execute_core(id, make, params).await;
        });
    }

    /// 分布式恢复态：resume 意图到达时把内存 Paused→Running 广播对齐（execute_core 内 checkpoint 也会处理）。
    fn local_resume_state(&self, id: JobId) {
        if let Some(mut job) = self.inner.table.get_mut(&id) {
            if job.status == JobStatus::Paused {
                job.status = JobStatus::Running;
                job.progress.rev += 1;
            }
        }
    }
}

/// 单例锁 key：kind [+ params.businessKey]。允许同种类按业务键并存多个单例。
fn singleton_key_of(kind: &str, params: &Value) -> String {
    let bk = params
        .get("businessKey")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty());
    match bk {
        Some(b) => format!("{kind}::{b}"),
        None => kind.to_string(),
    }
}
