//! 业务接触面：[`JobHandler`]（业务实现）+ [`JobContext`]（框架注入的句柄）+ 控制信号。
//!
//! 业务只实现 [`JobHandler`]，在 `run` 里通过 [`JobContext`] 上报进度、在循环埋 [`JobContext::checkpoint`]
//! 响应暂停/停止（方案 §7.4）。控制采用**协作式**（方案 §4）：[`JobManager`](crate::JobManager)
//! 经 `watch` 通道推最新意图，handler 在 checkpoint 安全点响应。

use async_trait::async_trait;
use serde_json::Value;

use crate::model::{JobCaps, JobError, JobPlan};
use crate::runtime::JobRuntime;
use std::sync::Arc;

/// 控制意图（watch 只保留「最新值」，天然去抖 + 幂等，方案 §4.2）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    /// 正常运行（放行 checkpoint）。
    Run,
    /// 暂停（checkpoint 挂起）。
    Pause,
    /// 停止（checkpoint 返回 Err，handler 走 `?` 优雅退出）。
    Cancel,
}

/// checkpoint 收到 Cancel 时返回的哨兵错误——handler 用 `?` 传播即优雅退出。
#[derive(Debug, Clone, Copy)]
pub struct JobCancelled;

impl std::fmt::Display for JobCancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "作业已被停止")
    }
}

impl std::error::Error for JobCancelled {}

impl From<JobCancelled> for JobError {
    fn from(_: JobCancelled) -> Self {
        JobError::cancelled()
    }
}

/// 日志级别（对齐 CmxErrCode 展示层）。
#[derive(Debug, Clone, Copy)]
pub enum LogLevel {
    Info,
    Warn,
    Error,
}

impl LogLevel {
    fn as_str(self) -> &'static str {
        match self {
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
        }
    }
}

/// 业务处理器：把「一次报表计算/记账/汇总」的真实逻辑写在这里（方案 §7.4、§9）。
///
/// 唯一需要业务配合框架的点是 [`JobContext::checkpoint`]——在长循环里周期调用，
/// 框架据此实现暂停/停止/心跳。不埋 checkpoint 的作业仍能跑完，但不可暂停/停止（见 [`JobCaps`]）。
#[async_trait]
pub trait JobHandler: Send + Sync {
    /// 业务种类标识（如 `"rpt.compute"`），与 `inventory` 注册一致，决定路由到本 handler。
    fn kind(&self) -> &'static str;

    /// 能力声明：可暂停？幂等？重启模式？框架据此决定 UI 与恢复策略。默认全能力。
    fn capabilities(&self) -> JobCaps {
        JobCaps::default()
    }

    /// 提交时预估执行计划（进度条基数 + 标题）。默认空计划（total=0，标题由框架兜底）。
    ///
    /// 应为纯/廉价操作（解析 params、算 total），不做重 I/O——重活留给 `run`。
    fn plan(&self, _params: &Value) -> Result<JobPlan, JobError> {
        Ok(JobPlan::default())
    }

    /// 执行作业主体。返回 `Ok(结果摘要 JSON)` → Completed；`Err(JobError)` → Failed；
    /// checkpoint 抛 [`JobCancelled`]（经 `?` 转 [`JobError`]）→ 由框架识别为 Cancelled。
    async fn run(&self, ctx: &JobContext, params: Value) -> Result<Value, JobError>;
}

/// 框架注入给 handler 的句柄：进度上报 + 控制响应（业务只调不实现）。
///
/// 克隆廉价（内部 `Arc`）。所有上报方法同步、非阻塞——更新内存权威快照并经 Hub 扇出 SSE。
#[derive(Clone)]
pub struct JobContext {
    rt: Arc<JobRuntime>,
}

impl JobContext {
    /// 由 [`JobManager`](crate::JobManager) 构造（内部用）。
    pub(crate) fn new(rt: Arc<JobRuntime>) -> Self {
        Self { rt }
    }

    /// 本作业 id。
    pub fn job_id(&self) -> i64 {
        self.rt.job_id()
    }

    /// **协作检查点**（方案 §4.2）：在长循环里周期调用。
    ///
    /// - 收到 `Cancel` → 返回 `Err(JobCancelled)`，handler 用 `?` 优雅退出。
    /// - 收到 `Pause` → **真正挂起**（`watch::changed().await`，零轮询、零 CPU），
    ///   直到 `Run`（恢复，零延迟唤醒）或 `Cancel`（退出）。
    /// - 收到 `Run` → 立即放行。
    pub async fn checkpoint(&self) -> Result<(), JobCancelled> {
        // 克隆一个接收端等待「意图变化」，无需 &mut self（handler 只持 &JobContext）。
        // 共享作业级持久 watch 通道，故 handler sleep 期间的 send 也能被这里读到。
        let mut rx = self.rt.control_receiver();
        let mut was_paused = false;
        loop {
            // borrow_and_update：标记当前值已读，使随后 changed() 仅等「下一次」变化。
            let intent = *rx.borrow_and_update();
            match intent {
                Control::Cancel => return Err(JobCancelled),
                Control::Run => {
                    if was_paused {
                        self.rt.leave_paused();
                    }
                    return Ok(());
                }
                Control::Pause => {
                    if !was_paused {
                        self.rt.enter_paused();
                        was_paused = true;
                    }
                    // 挂起直至下一次控制意图变化；sender 被 drop（作业被强制清理）视为取消。
                    if rx.changed().await.is_err() {
                        return Err(JobCancelled);
                    }
                }
            }
        }
    }

    /// 设置当前阶段（多阶段作业：装载/求值/落库…）。
    pub fn set_phase(&self, index: u32, total: u32, name: impl Into<String>) {
        self.rt.set_phase(index, total, name.into());
    }

    /// 设置/修正总明细基数（plan 未预估或运行中才知道总数时）。
    pub fn set_total(&self, total: u64) {
        self.rt.set_total(total);
    }

    /// 设置一句话人读状态消息。
    pub fn message(&self, text: impl Into<String>) {
        self.rt.set_message(text.into());
    }

    /// 注册一个明细行（初始 Queued）。key 为稳定业务键（后续 upsert 寻址）。
    pub fn add_item(&self, key: impl Into<String>, label: impl Into<String>) {
        self.rt.add_item(key.into(), label.into());
    }

    /// 明细行 → 处理中。
    pub fn item_running(&self, key: &str, detail: impl Into<String>) {
        self.rt
            .update_item(key, crate::model::ItemState::Running, detail.into(), None);
    }

    /// 明细行 → 成功（附耗时），成功计数 +1。
    pub fn item_ok(&self, key: &str, elapsed_ms: u64) {
        self.rt.update_item(
            key,
            crate::model::ItemState::Ok,
            format!("{elapsed_ms}ms"),
            Some(true),
        );
    }

    /// 明细行 → 失败（附错误文本），失败计数 +1。
    pub fn item_fail(&self, key: &str, err: impl std::fmt::Display) {
        self.rt.update_item(
            key,
            crate::model::ItemState::Failed,
            err.to_string(),
            Some(false),
        );
    }

    /// 总进度 +n（推进「已完成」计数，触发 progress 事件）。
    pub fn progress_inc(&self, n: u64) {
        self.rt.progress_inc(n);
    }

    /// 记业务日志（同步扇出 log 事件；M1 不落库）。
    pub fn log(&self, level: LogLevel, text: impl Into<String>) {
        self.rt.log(level.as_str(), &text.into());
    }

    /// INFO 日志便捷方法。
    pub fn info(&self, text: impl Into<String>) {
        self.log(LogLevel::Info, text);
    }

    /// WARN 日志便捷方法。
    pub fn warn(&self, text: impl Into<String>) {
        self.log(LogLevel::Warn, text);
    }
}
