//! SSE 事件与按 job_id 扇出的事件枢纽。
//!
//! 母版 = `cmx-ai::session_registry`：
//!   - [`JobEvent`] ≈ `AiSseEvent`（event_name + 已序列化 payload），handler 转 axum `Event`。
//!   - [`JobEventHub`] ≈ `SessionRegistry`：`{job_id → Vec<前端 mpsc sender>}`，
//!     `subscribe(id)→Receiver` / `broadcast(id, ev)`，支持一个作业多前端订阅（多标签页）。
//!
//! 与 AI 的差异：这里的 payload 类型更专（进度/明细/日志/状态），且 [`JobEventHub::broadcast`]
//! 对无订阅者的作业静默丢弃（后端自发起、无人监控的作业照样跑，事件不落地——M1 不持久化事件流）。

use std::sync::{Arc, Mutex};

use dashmap::DashMap;
use serde::Serialize;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tracing::{debug, trace};

use crate::model::{JobError, JobStatus, ProgressItem, ProgressSnapshot};

/// 推给前端的一个 SSE 事件（event 字段 = 类型名，data = JSON 字符串）。
#[derive(Debug, Clone)]
pub struct JobEvent {
    /// SSE 帧 `event:` 字段（前端 `addEventListener` 用）。
    pub event_name: &'static str,
    /// 事件载荷 JSON 字符串（写入 SSE `data:`）。
    pub payload: String,
}

impl JobEvent {
    /// 通用构造：序列化任意载荷。
    pub fn new(event_name: &'static str, payload: impl Serialize) -> Self {
        let payload = serde_json::to_string(&payload).unwrap_or_else(|_| "{}".into());
        Self {
            event_name,
            payload,
        }
    }

    /// `snapshot` —— 订阅建立时首帧，携带完整快照 + 当前状态（方案 §5.2）。
    pub fn snapshot(status: JobStatus, snap: &ProgressSnapshot) -> Self {
        Self::new(
            "snapshot",
            serde_json::json!({ "status": status.as_str(), "progress": snap }),
        )
    }

    /// `state` —— 状态机跃迁。
    pub fn state(status: JobStatus, rev: u64) -> Self {
        Self::new(
            "state",
            serde_json::json!({ "status": status.as_str(), "rev": rev }),
        )
    }

    /// `progress` —— 进度推进（去抖后的快照头部字段，不含 items 全量）。
    pub fn progress(snap: &ProgressSnapshot) -> Self {
        Self::new(
            "progress",
            serde_json::json!({
                "phase": snap.phase,
                "phaseIndex": snap.phase_index,
                "phaseTotal": snap.phase_total,
                "done": snap.done,
                "total": snap.total,
                "ok": snap.ok,
                "failed": snap.failed,
                "percent": snap.percent(),
                "message": snap.message,
                "etaMs": snap.eta_ms,
                "rev": snap.rev,
            }),
        )
    }

    /// `item` —— 单个明细行变化（前端按 key upsert）。
    pub fn item(it: &ProgressItem) -> Self {
        Self::new("item", it)
    }

    /// `log` —— 业务日志。
    pub fn log(level: &str, text: &str, at: i64) -> Self {
        Self::new(
            "log",
            serde_json::json!({ "level": level, "text": text, "at": at }),
        )
    }

    /// `result` —— 成功摘要。
    pub fn result(data: &serde_json::Value) -> Self {
        Self::new("result", data)
    }

    /// `error` —— 失败明细。
    pub fn error(err: &JobError) -> Self {
        Self::new("error", err)
    }

    /// `done` —— 流终结（任何终态后），提示前端可关闭 EventSource。
    pub fn done() -> Self {
        Self::new("done", serde_json::json!({}))
    }
}

/// 单作业订阅者集合。std Mutex：send/push 非阻塞、不跨 await 持锁（母版同 cmx-ai）。
type Subscribers = Vec<UnboundedSender<JobEvent>>;

/// 按 job_id 扇出事件的枢纽（全局单例的一部分，随 JobManager 持有）。
#[derive(Default)]
pub struct JobEventHub {
    /// `{job_id → 订阅 sender 列表}`。
    subscriptions: DashMap<i64, Arc<Mutex<Subscribers>>>,
}

impl JobEventHub {
    pub fn new() -> Self {
        Self {
            subscriptions: DashMap::new(),
        }
    }

    /// 订阅指定作业的事件流，返回 receiver。
    ///
    /// 同一作业可多次订阅（多标签页/多监控器），各自独立 receiver。
    /// 注意：调用方（cmx-job-api handler）在 subscribe 后应立即补发一帧 `snapshot`
    /// （从 JobManager 取当前快照），保证新连接拿到全貌——本 Hub 不缓存快照。
    pub fn subscribe(&self, job_id: i64) -> UnboundedReceiver<JobEvent> {
        let (tx, rx) = mpsc::unbounded_channel();
        let subs = self
            .subscriptions
            .entry(job_id)
            .or_insert_with(|| Arc::new(Mutex::new(Vec::new())))
            .clone();
        subs.lock().expect("订阅锁中毒").push(tx);
        debug!(job_id, "前端订阅作业事件流");
        rx
    }

    /// 向指定作业的所有订阅者广播一个事件（无订阅者静默丢弃）。
    pub fn broadcast(&self, job_id: i64, event: JobEvent) {
        let subs = match self.subscriptions.get(&job_id) {
            Some(arc) => arc.clone(),
            None => {
                trace!(job_id, "广播目标作业无订阅者，丢弃事件");
                return;
            }
        };
        let mut guard = match subs.lock() {
            Ok(g) => g,
            Err(e) => {
                tracing::warn!(job_id, error = %e, "订阅锁中毒，跳过广播");
                return;
            }
        };
        // 逆序删除失效订阅（receiver 已 drop）+ 广播存活者。
        let mut i = guard.len();
        while i > 0 {
            i -= 1;
            if guard[i].send(event.clone()).is_err() {
                guard.remove(i);
            }
        }
    }

    /// 清理指定作业的全部订阅（作业删除时调用）。
    pub fn clear(&self, job_id: i64) {
        self.subscriptions.remove(&job_id);
        debug!(job_id, "清理作业订阅");
    }

    /// 当前作业是否有前端在监控。
    pub fn has_subscribers(&self, job_id: i64) -> bool {
        self.subscriptions
            .get(&job_id)
            .map(|arc| !arc.lock().map(|g| g.is_empty()).unwrap_or(true))
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn subscribe_and_broadcast() {
        let hub = JobEventHub::new();
        let mut rx = hub.subscribe(1);
        hub.broadcast(1, JobEvent::log("INFO", "hello", 0));
        let ev = rx.recv().await.expect("应收到事件");
        assert_eq!(ev.event_name, "log");
        assert!(ev.payload.contains("hello"));
    }

    #[tokio::test]
    async fn no_subscriber_is_noop() {
        let hub = JobEventHub::new();
        hub.broadcast(999, JobEvent::done()); // 不 panic
        assert!(!hub.has_subscribers(999));
    }

    #[tokio::test]
    async fn multi_subscriber_fanout() {
        let hub = JobEventHub::new();
        let mut a = hub.subscribe(7);
        let mut b = hub.subscribe(7);
        hub.broadcast(7, JobEvent::state(JobStatus::Running, 1));
        assert!(a.recv().await.is_some());
        assert!(b.recv().await.is_some());
    }
}
