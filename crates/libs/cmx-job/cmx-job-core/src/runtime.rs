//! 单作业运行态：进度快照的权威载体 + 控制通道 + 事件扇出桥。
//!
//! [`JobRuntime`] 是 [`JobContext`](crate::JobContext) 背后的实现：context 的每个上报方法
//! 都落到这里——更新共享作业表 [`JobTable`] 里该作业的 [`ProgressSnapshot`]（bump rev），
//! 再经 [`JobEventHub`] 广播对应 SSE 事件。控制信号经 `watch::Sender<Control>` 下发给 checkpoint。

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use dashmap::DashMap;
use tokio::sync::watch;

use crate::context::Control;
use crate::event::{JobEvent, JobEventHub};
use crate::model::{ItemState, Job, JobId, JobStatus, ProgressItem};

/// 内存态作业表（进程内热态；持久化经 [`crate::JobStore`] 另行写穿）。
pub type JobTable = Arc<DashMap<JobId, Job>>;

/// 当前 epoch 毫秒（普通 Rust，`SystemTime` 可用；非 workflow 脚本环境）。
pub fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Single running job's runtime state (one per Running job, lives with the handler).
pub struct JobRuntime {
    job_id: JobId,
    /// 共享作业表（就地更新本作业的快照/状态）。
    store: JobTable,
    /// 事件枢纽（广播进度/明细/日志/状态）。
    hub: Arc<JobEventHub>,
    /// 控制意图接收端（持久保活：只要 JobRuntime 存活，watch 通道就不关闭，
    /// 使 `send` 在 handler sleep 期间也能成功——checkpoint 侧 clone 出瞬时 rx 等待变化）。
    control_rx: watch::Receiver<Control>,
    /// 起算时刻（ETA 估算）。
    started_ms: AtomicI64,
}

impl JobRuntime {
    pub(crate) fn new(
        job_id: JobId,
        store: JobTable,
        hub: Arc<JobEventHub>,
        control_rx: watch::Receiver<Control>,
    ) -> Self {
        Self {
            job_id,
            store,
            hub,
            control_rx,
            started_ms: AtomicI64::new(now_ms()),
        }
    }

    pub fn job_id(&self) -> JobId {
        self.job_id
    }

    /// checkpoint 侧克隆一个接收端等待控制意图变化（共享同一 watch 通道）。
    pub(crate) fn control_receiver(&self) -> watch::Receiver<Control> {
        self.control_rx.clone()
    }

    /// 就地修改本作业快照并 bump rev；闭包外统一广播由各方法自理。
    fn with_snapshot<R>(&self, f: impl FnOnce(&mut crate::model::ProgressSnapshot) -> R) -> R {
        let mut job = self
            .store
            .get_mut(&self.job_id)
            .expect("运行态作业必在作业表中");
        job.progress.rev += 1;
        f(&mut job.progress)
    }

    // ── 状态跃迁（仅 Paused↔Running；生命周期终态由 Manager 掌管）──

    /// checkpoint 进入暂停：Running → Paused（仅当当前为 Running 才改，避免覆盖 Cancelling）。
    pub(crate) fn enter_paused(&self) {
        let rev = {
            let mut job = match self.store.get_mut(&self.job_id) {
                Some(j) => j,
                None => return,
            };
            if job.status != JobStatus::Running {
                return;
            }
            job.status = JobStatus::Paused;
            job.progress.rev += 1;
            job.progress.rev
        };
        self.hub
            .broadcast(self.job_id, JobEvent::state(JobStatus::Paused, rev));
    }

    /// checkpoint 恢复：Paused → Running（仅当当前为 Paused）。
    pub(crate) fn leave_paused(&self) {
        let rev = {
            let mut job = match self.store.get_mut(&self.job_id) {
                Some(j) => j,
                None => return,
            };
            if job.status != JobStatus::Paused {
                return;
            }
            job.status = JobStatus::Running;
            job.progress.rev += 1;
            job.progress.rev
        };
        self.hub
            .broadcast(self.job_id, JobEvent::state(JobStatus::Running, rev));
    }

    // ── 进度上报（context 各方法的落点）──

    pub(crate) fn set_phase(&self, index: u32, total: u32, name: String) {
        self.with_snapshot(|s| {
            s.phase = name;
            s.phase_index = index;
            s.phase_total = total;
        });
        self.emit_progress();
    }

    pub(crate) fn set_total(&self, total: u64) {
        self.with_snapshot(|s| s.total = total);
        self.emit_progress();
    }

    pub(crate) fn set_message(&self, text: String) {
        self.with_snapshot(|s| s.message = text);
        self.emit_progress();
    }

    pub(crate) fn add_item(&self, key: String, label: String) {
        let item = ProgressItem::new(key, label);
        self.with_snapshot(|s| s.items.push(item.clone()));
        self.hub.broadcast(self.job_id, JobEvent::item(&item));
    }

    /// 更新一行明细状态（未预注册则即时补建），广播 item 事件。
    /// `tally`: Some(true)=成功计数+1，Some(false)=失败计数+1，None=不计（如置 Running）。
    pub(crate) fn update_item(&self, key: &str, state: ItemState, detail: String, tally: Option<bool>) {
        let updated = self.with_snapshot(|s| {
            match tally {
                Some(true) => s.ok += 1,
                Some(false) => s.failed += 1,
                None => {}
            }
            if let Some(it) = s.items.iter_mut().find(|i| i.key == key) {
                it.state = state;
                it.detail = detail.clone();
                Some(it.clone())
            } else {
                // 未预注册的行：即时补建（宽容业务未 add_item 的情况）。
                let mut it = ProgressItem::new(key.to_string(), key.to_string());
                it.state = state;
                it.detail = detail.clone();
                s.items.push(it.clone());
                Some(it)
            }
        });
        if let Some(it) = updated {
            self.hub.broadcast(self.job_id, JobEvent::item(&it));
        }
    }

    pub(crate) fn progress_inc(&self, n: u64) {
        self.with_snapshot(|s| {
            s.done += n;
            // ETA：按已用时 / 已完成 外推剩余。
            if s.total > 0 && s.done > 0 && s.done <= s.total {
                let elapsed = (now_ms() - self.started_ms.load(Ordering::Relaxed)).max(0) as u64;
                let per = elapsed / s.done.max(1);
                s.eta_ms = Some(per * (s.total - s.done));
            }
        });
        self.emit_progress();
    }

    pub(crate) fn log(&self, level: &str, text: &str) {
        self.hub
            .broadcast(self.job_id, JobEvent::log(level, text, now_ms()));
    }

    /// 广播一帧 progress（读当前快照）。
    fn emit_progress(&self) {
        if let Some(job) = self.store.get(&self.job_id) {
            self.hub
                .broadcast(self.job_id, JobEvent::progress(&job.progress));
        }
    }
}
