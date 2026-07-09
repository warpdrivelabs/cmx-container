//! 前端订阅路由表 + 待处理 question/permission id 管理 + 活跃生成锁 + 超时定时器。
//!
//! 维护四类状态（均为内存，一期不持久化）：
//! - **订阅路由表**：`{opencode ses_* → Vec<前端 mpsc sender>}`。sse_relay 把翻译后的
//!   [`AiSseEvent`] 按 sessionID 广播到该 session 的所有活跃前端连接（支持多标签页）。
//! - **pending id 表**：`{ses_* → 当前待处理的 que_*/per_* id + 超时任务句柄}`。answer/approval
//!   接口据此转发到正确的 OpenCode 端点；超时未答时定时器自动调 OpenCode reject。
//! - **活跃生成锁**：`{ses_* → 是否有活跃生成流}`。同一 session 仅允许一条活跃生成流，
//!   `send_message` 并发时第二条返回 409（文档 4.7 并发冲突）。

use std::sync::{Arc, Mutex};

use dashmap::DashMap;
use serde::Serialize;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::sync::oneshot;
use tracing::{debug, trace, warn};

use crate::types::*;

/// cmx-ai 推给前端的 SSE 事件（relay 翻译产出，handler 转 axum Event）。
///
/// `event_name` 即 SSE 帧的 `event:` 字段（`text_delta` / `ask_user` / ...），
/// `payload` 是已序列化好的 JSON 字符串（直接写入 SSE `data:`）。
#[derive(Debug, Clone)]
pub struct AiSseEvent {
    /// SSE 事件类型名（前端 `addEventListener` 用）。
    pub event_name: &'static str,
    /// 事件载荷 JSON 字符串。
    pub payload: String,
}

impl AiSseEvent {
    /// 构造一个 SSE 事件。
    pub fn new(event_name: &'static str, payload: impl Serialize) -> Self {
        let payload = serde_json::to_string(&payload).unwrap_or_else(|_| "{}".into());
        Self { event_name, payload }
    }

    /// `text_delta` 事件。
    pub fn text_delta(content: impl Into<String>) -> Self {
        Self::new("text_delta", &TextDeltaEvent { content: content.into() })
    }

    /// `reasoning_delta` 事件。
    pub fn reasoning_delta(content: impl Into<String>) -> Self {
        Self::new("reasoning_delta", &ReasoningDeltaEvent { content: content.into() })
    }

    /// `tool_call` 事件（普通工具，仅 tool + state）。
    pub fn tool_call(tool: impl Into<String>, state: impl Into<String>) -> Self {
        Self::new(
            "tool_call",
            &ToolCallEvent {
                tool: tool.into(),
                part_id: String::new(),
                state: state.into(),
                input: None,
                output: None,
                metadata: None,
            },
        )
    }

    /// `tool_call` 事件（完整工具调用事件，携带 input/output/metadata）。
    /// 用于 question 工具 completed 态（带 questions + answers）等需要透传数据的场景。
    pub fn tool_call_full(event: ToolCallEvent) -> Self {
        Self::new("tool_call", &event)
    }

    /// `json_chunk` 事件：渐进 JSON 片段。
    pub fn json_chunk(event: JsonChunkEvent) -> Self {
        Self::new("json_chunk", &event)
    }

    /// `result` 事件。
    pub fn result(event: ResultEvent) -> Self {
        Self::new("result", &event)
    }

    /// `error` 事件。
    pub fn error(message: impl Into<String>, code: Option<u16>) -> Self {
        Self::new("error", &ErrorEvent { message: message.into(), code })
    }

    /// `done` 事件。
    pub fn done() -> Self {
        Self::new("done", &DoneEvent {})
    }
}

/// 单个 session 的订阅集合（前端连接的 sender 列表）。
///
/// 用 `std::sync::Mutex` 而非 `tokio::sync::Mutex`：push / send 均为非阻塞同步操作，
/// 不会跨 await 持锁，std Mutex 更轻量且避免 try_lock 竞争回退。
type Subscribers = Vec<UnboundedSender<AiSseEvent>>;

/// 待处理请求条目（询问或审批），仅记录 OpenCode 请求 id。
///
/// answer/approve 接口据此转发到 OpenCode；会话结束时清理。
/// 无超时（对齐 OpenCode 原生行为：question/permission 无限等待）。
#[derive(Debug)]
struct PendingEntry {
    /// OpenCode 请求 id（`que_*` 或 `per_*`）。
    request_id: String,
}

/// 待处理的隐式上下文请求条目（插件工具 → 前端回传桥接）。
///
/// 工具发起 context-request 后挂起在 oneshot::Receiver 上；
/// 前端回传 context-response 时取出 Sender 投递数据，工具解除挂起。
struct PendingContextEntry {
    /// 所属 session（purge 时按 session 批量清理用）。
    session_id: String,
    /// oneshot 发送端——resolve 时 send(data)，接收端在 handler 里 await。
    tx: oneshot::Sender<serde_json::Value>,
}

/// 会话订阅 + pending id 注册表 + 活跃生成锁。
///
/// 线程安全（`DashMap` + 内部 `Mutex`），全局单例（经 [`crate::init_ai_subsystem`] 初始化）。
pub struct SessionRegistry {
    /// `{opencode ses_* → 订阅 sender 列表}`。
    subscriptions: DashMap<String, Arc<Mutex<Subscribers>>>,
    /// `{ses_* → 当前待处理的询问（que_*）+ 超时定时器}`。
    pending_questions: DashMap<String, PendingEntry>,
    /// `{ses_* → 当前待处理的审批（per_*）+ 超时定时器}`。
    pending_permissions: DashMap<String, PendingEntry>,
    /// `{ctx_* → 隐式上下文请求（插件工具 ↔ 前端桥接）}`。
    pending_context: DashMap<String, PendingContextEntry>,
    /// `{ses_* → 是否有活跃生成流}`（session 级并发锁，文档 4.7）。
    active_streams: DashMap<String, ()>,
}

impl SessionRegistry {
    /// 创建空注册表。
    pub fn new() -> Self {
        Self {
            subscriptions: DashMap::new(),
            pending_questions: DashMap::new(),
            pending_permissions: DashMap::new(),
            pending_context: DashMap::new(),
            active_streams: DashMap::new(),
        }
    }

    /// 订阅指定 session 的事件流，返回 receiver。
    ///
    /// 同一 session 可多次订阅（多标签页）；每个订阅独立 receiver。
    pub fn subscribe(&self, session_id: &str) -> UnboundedReceiver<AiSseEvent> {
        let (tx, rx) = mpsc::unbounded_channel();
        let subs = self
            .subscriptions
            .entry(session_id.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(Vec::new())))
            .clone();
        // std::sync::Mutex，push 是同步操作，lock().unwrap() 安全（不会 panic：仅中毒场景，本项目无）。
        subs.lock().expect("订阅锁中毒").push(tx);
        debug!(session_id, "前端订阅 AI 事件流");
        rx
    }

    /// 向指定 session 的所有订阅者广播一个事件。
    ///
    /// 广播失败的 subscriber（receiver 已 drop）会被清理。
    pub fn broadcast(&self, session_id: &str, event: AiSseEvent) {
        let subs = match self.subscriptions.get(session_id) {
            Some(arc) => arc.clone(),
            None => {
                trace!(session_id, "广播目标 session 无订阅者，丢弃事件");
                return;
            }
        };
        let mut guard = match subs.lock() {
            Ok(g) => g,
            Err(e) => {
                tracing::warn!(session_id, error = %e, "订阅锁中毒，跳过广播");
                return;
            }
        };
        // 逆序删除失效订阅 + 广播（保留存活 sender）。
        let mut i = guard.len();
        while i > 0 {
            i -= 1;
            if guard[i].send(event.clone()).is_err() {
                guard.remove(i);
            }
        }
    }

    /// 注销指定 session 的全部订阅（会话删除/前端全部断开时调用）。
    pub fn clear_subscribers(&self, session_id: &str) {
        self.subscriptions.remove(session_id);
        debug!(session_id, "清理 session 订阅");
    }

    /// 登记当前 session 待处理的询问（仅记录 id，供 answer 接口转发）。
    ///
    /// 无超时 —— 对齐 OpenCode 原生行为：question 无限等待直到用户回答/会话结束。
    pub fn register_pending_question(
        &self,
        session_id: &str,
        question_id: impl Into<String>,
    ) {
        self.pending_questions.insert(
            session_id.to_string(),
            PendingEntry {
                request_id: question_id.into(),
            },
        );
    }

    /// 取出并清除当前 session 待处理的询问。
    ///
    /// 返回 `None` 表示无待处理询问（前端重复回答或会话已结束）。
    pub fn take_pending_question(&self, session_id: &str) -> Option<String> {
        self.pending_questions
            .remove(session_id)
            .map(|(_, v)| v.request_id)
    }

    /// 登记当前 session 待处理的审批（仅记录 id，供 approval 接口转发）。
    ///
    /// 无超时 —— 对齐 OpenCode 原生行为：permission 无限等待。
    pub fn register_pending_permission(
        &self,
        session_id: &str,
        permission_id: impl Into<String>,
    ) {
        self.pending_permissions.insert(
            session_id.to_string(),
            PendingEntry {
                request_id: permission_id.into(),
            },
        );
    }

    /// 取出并清除当前 session 待处理的审批。
    pub fn take_pending_permission(&self, session_id: &str) -> Option<String> {
        self.pending_permissions
            .remove(session_id)
            .map(|(_, v)| v.request_id)
    }

    // ── 隐式上下文回传桥接（插件工具 ↔ 前端）──

    /// 登记一次隐式上下文请求，返回 oneshot::Receiver 供 handler await。
    ///
    /// 工具发起 context-request 后挂起在 receiver 上；前端回传 context-response 时
    /// [`resolve_context_request`] 投递数据，receiver 解除阻塞。
    /// request_id 由插件生成（`ctx_*`），按 id 而非 session 做 key（更精确）。
    pub fn register_context_request(
        &self,
        session_id: &str,
        request_id: impl Into<String>,
    ) -> oneshot::Receiver<serde_json::Value> {
        let (tx, rx) = oneshot::channel();
        self.pending_context.insert(
            request_id.into(),
            PendingContextEntry {
                session_id: session_id.to_string(),
                tx,
            },
        );
        rx
    }

    /// 投递前端回传的上下文数据，解除对应工具的挂起。
    ///
    /// 返回 `true` 表示找到并投递成功；`false` 表示无此 pending（已超时清理或不存在）。
    pub fn resolve_context_request(
        &self,
        request_id: &str,
        data: serde_json::Value,
    ) -> bool {
        if let Some((_, entry)) = self.pending_context.remove(request_id) {
            // send 失败说明 receiver 已 drop（handler 超时已返回），忽略即可。
            let _ = entry.tx.send(data);
            true
        } else {
            false
        }
    }

    // ── session 级活跃生成锁（文档 4.7 并发冲突）──

    /// 尝试获取 session 的活跃生成锁。
    ///
    /// 返回 `true` 表示获取成功（当前 session 无活跃生成流，已登记为活跃）；
    /// `false` 表示已有活跃流在跑，调用方应返回 409。
    pub fn try_acquire_session(&self, session_id: &str) -> bool {
        // entry().or_insert 仅在 key 不存在时插入，返回 Vacant/Occupied 的引用。
        use dashmap::mapref::entry::Entry;
        match self.active_streams.entry(session_id.to_string()) {
            Entry::Vacant(v) => {
                v.insert(());
                debug!(session_id, "获取 session 活跃生成锁");
                true
            }
            Entry::Occupied(_) => {
                warn!(session_id, "session 已有活跃生成流，拒绝并发（409）");
                false
            }
        }
    }

    /// 释放 session 的活跃生成锁（生成结束 idle/error/abort 时调用）。
    pub fn release_session(&self, session_id: &str) {
        if self.active_streams.remove(session_id).is_some() {
            debug!(session_id, "释放 session 活跃生成锁");
        }
    }

    /// session 当前是否有活跃生成流。
    pub fn is_session_active(&self, session_id: &str) -> bool {
        self.active_streams.contains_key(session_id)
    }

    /// 清理指定 session 的所有状态（订阅 + pending id + 超时定时器 + 活跃锁 + 上下文桥接）。
    pub fn purge(&self, session_id: &str) {
        self.clear_subscribers(session_id);
        self.pending_questions.remove(session_id);
        self.pending_permissions.remove(session_id);
        self.active_streams.remove(session_id);
        // pending_context 按 request_id 做 key，需遍历找出属于该 session 的条目清理。
        let stale: Vec<String> = self
            .pending_context
            .iter()
            .filter(|e| e.session_id == session_id)
            .map(|e| e.key().clone())
            .collect();
        for rid in stale {
            self.pending_context.remove(&rid);
        }
        debug!(session_id, "清理 session 全部状态");
    }
}

impl Default for SessionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn subscribe_and_broadcast() {
        let reg = SessionRegistry::new();
        let mut rx = reg.subscribe("ses_test1");
        reg.broadcast(
            "ses_test1",
            AiSseEvent::text_delta("hello"),
        );
        let ev = rx.recv().await.expect("应收到事件");
        assert_eq!(ev.event_name, "text_delta");
        assert!(ev.payload.contains("hello"));
    }

    #[tokio::test]
    async fn no_subscriber_broadcast_is_noop() {
        let reg = SessionRegistry::new();
        // 无订阅者广播不应 panic。
        reg.broadcast("ses_none", AiSseEvent::done());
    }

    #[tokio::test]
    async fn pending_question_lifecycle() {
        let reg = SessionRegistry::new();
        let timer = tokio::spawn(async {});
        reg.register_pending_question("ses_t2", "que_abc", timer);
        assert_eq!(reg.take_pending_question("ses_t2"), Some("que_abc".into()));
        assert_eq!(reg.take_pending_question("ses_t2"), None);
    }

    #[tokio::test]
    async fn purge_clears_all() {
        let reg = SessionRegistry::new();
        let timer = tokio::spawn(async {});
        reg.register_pending_permission("ses_t3", "per_xyz", timer);
        reg.purge("ses_t3");
        assert_eq!(reg.take_pending_permission("ses_t3"), None);
    }

    #[tokio::test]
    async fn active_lock_acquire_release() {
        let reg = SessionRegistry::new();
        // 首次获取成功。
        assert!(reg.try_acquire_session("ses_lock1"));
        assert!(reg.is_session_active("ses_lock1"));
        // 第二次并发获取失败（409 语义）。
        assert!(!reg.try_acquire_session("ses_lock1"));
        // 释放后可再次获取。
        reg.release_session("ses_lock1");
        assert!(!reg.is_session_active("ses_lock1"));
        assert!(reg.try_acquire_session("ses_lock1"));
        reg.release_session("ses_lock1");
    }

    #[tokio::test]
    async fn different_sessions_independent_locks() {
        let reg = SessionRegistry::new();
        assert!(reg.try_acquire_session("ses_a"));
        assert!(reg.try_acquire_session("ses_b")); // 不同 session 互不影响
        reg.release_session("ses_a");
        reg.release_session("ses_b");
    }

    #[tokio::test]
    async fn purge_releases_active_lock() {
        let reg = SessionRegistry::new();
        reg.try_acquire_session("ses_purge");
        reg.purge("ses_purge");
        assert!(!reg.is_session_active("ses_purge"));
    }
}
