//! 作业领域模型与状态机（语义中立，DB-free）。
//!
//! 所有长任务共用的一等公民 [`Job`]，其生命周期由 [`JobStatus`] 状态机刻画，
//! 进度由「完整快照 [`ProgressSnapshot`] + 明细行 [`ProgressItem`]」双层表达（方案 §3、§5）。

use serde::{Deserialize, Serialize};

/// 作业唯一标识（M1 进程内自增；未来迁 bigint 后端铸号，前端无感）。
pub type JobId = i64;

// ───────────────────────── 状态机（方案 §3.1）─────────────────────────

/// 作业状态机。终态：Completed / Cancelled；Failed 可 restart。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    /// 已入队，等待 worker 空闲。
    Pending,
    /// worker 正在执行 handler。
    Running,
    /// handler 在 checkpoint 挂起，占用 worker 槽但不推进。
    Paused,
    /// 已发停止信号，handler 正在响应/清理（协作式，非瞬时）。
    Cancelling,
    /// 已停止（终态）。
    Cancelled,
    /// handler 正常返回（终态）。
    Completed,
    /// handler 返回错误 / panic（终态，可 restart）。
    Failed,
}

impl JobStatus {
    /// 是否终态（不再推进、可安全从内存清理/删除）。
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Cancelled | Self::Completed | Self::Failed)
    }

    /// 是否活跃（占用 worker 槽：running/paused/cancelling）。
    pub fn is_active(self) -> bool {
        matches!(self, Self::Running | Self::Paused | Self::Cancelling)
    }

    /// SSE `state` 事件的字符串形式。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Paused => "paused",
            Self::Cancelling => "cancelling",
            Self::Cancelled => "cancelled",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }
}

// ───────────────────────── 进度模型（方案 §5）─────────────────────────

/// 明细行状态（如报表计算里每张报表一行）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemState {
    /// 排队未开始。
    Queued,
    /// 处理中。
    Running,
    /// 成功。
    Ok,
    /// 失败。
    Failed,
    /// 跳过（如断点续跑已完成项）。
    Skipped,
}

impl ItemState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Ok => "ok",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
        }
    }
}

/// 进度明细单元：按 [`ProgressItem::key`] 寻址 upsert（稳定业务键，如报表 code）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressItem {
    /// 稳定业务键（upsert 寻址用）。
    pub key: String,
    /// 显示名。
    pub label: String,
    /// 明细状态。
    pub state: ItemState,
    /// 细节文本（"1.2s" / "除零错误@B12"）。
    #[serde(default)]
    pub detail: String,
}

impl ProgressItem {
    pub fn new(key: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            state: ItemState::Queued,
            detail: String::new(),
        }
    }
}

/// 某一刻的完整进度视图（事实源，方案 §5.2「快照优先」）。
///
/// 新订阅者连上先收一份全量（SSE `snapshot` 首帧），之后收 `progress`/`item` 增量。
/// [`ProgressSnapshot::rev`] 单调递增，前端据此去重/抗乱序。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProgressSnapshot {
    /// 当前阶段名（"装载模板" / "求值" / "落库"）。
    #[serde(default)]
    pub phase: String,
    /// 第几阶段（1-based）。
    #[serde(default)]
    pub phase_index: u32,
    /// 总阶段数。
    #[serde(default)]
    pub phase_total: u32,
    /// 已完成明细数。
    #[serde(default)]
    pub done: u64,
    /// 总明细数（0 表示未知/未定基数）。
    #[serde(default)]
    pub total: u64,
    /// 成功数。
    #[serde(default)]
    pub ok: u64,
    /// 失败数。
    #[serde(default)]
    pub failed: u64,
    /// 一句话人读状态。
    #[serde(default)]
    pub message: String,
    /// 预估剩余毫秒（可选）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eta_ms: Option<u64>,
    /// 明细行（M1 全量保留；大作业未来裁剪）。
    #[serde(default)]
    pub items: Vec<ProgressItem>,
    /// 单调版本号（前端去重/抗乱序）。
    #[serde(default)]
    pub rev: u64,
}

impl ProgressSnapshot {
    /// 百分比（0–100，整数）。total 为 0 时返回 0。
    pub fn percent(&self) -> u32 {
        if self.total == 0 {
            return 0;
        }
        ((self.done.min(self.total) as f64 / self.total as f64) * 100.0).round() as u32
    }
}

// ───────────────────────── 作业实体（方案 §3.2）─────────────────────────

/// 作业来源：前端发起 or 后端自发起（方案 §11）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum JobOrigin {
    /// 前端用户发起（携带发起人）。
    Frontend {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        user: Option<String>,
    },
    /// 后端自发起（定时器 / 事件总线 / 其它服务），携带触发源标识。
    Backend { trigger: String },
}

impl Default for JobOrigin {
    fn default() -> Self {
        Self::Backend {
            trigger: "unknown".into(),
        }
    }
}

/// 作业失败明细（对齐 CmxErrCode 体系：code + message + 结构化 violations）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobError {
    /// 业务错误码（对齐 ErrCode：500 内部 / 422 校验 / 409 冲突 / 499 取消…）。
    pub code: u16,
    /// 人读消息。
    pub message: String,
    /// 结构化违规明细（对齐 DCT/DOC violations，可空）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub violations: Vec<serde_json::Value>,
}

impl JobError {
    pub fn new(code: u16, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            violations: Vec::new(),
        }
    }

    /// 作业被取消的错误（499，对齐 cmx-ai 中断码语义）。
    pub fn cancelled() -> Self {
        Self::new(499, "作业已被停止")
    }
}

/// 作业主体（内存态；未来 store-pg 落库同构映射）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    /// 唯一 id。
    pub id: JobId,
    /// 业务种类（决定 Handler），如 "rpt.compute"。
    pub kind: String,
    /// 人读标题。
    pub title: String,
    /// 业务入参（org/period/report_codes…）。
    pub params: serde_json::Value,
    /// 当前状态。
    pub status: JobStatus,
    /// 当前完整进度快照。
    pub progress: ProgressSnapshot,
    /// 成功结果摘要。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    /// 失败明细。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<JobError>,
    /// 调度优先级（预留，默认 0；越大越先，方案 §12）。
    #[serde(default)]
    pub priority: i16,
    /// 来源。
    #[serde(default)]
    pub origin: JobOrigin,
    /// 多租户归属（鉴权/过滤）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub org_id: Option<i64>,
    /// 发起人 id。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_by: Option<i64>,
    /// 创建时刻（epoch ms）。
    pub created_at: i64,
    /// 开始执行时刻。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<i64>,
    /// 结束时刻。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<i64>,
}

/// 提交一个作业的入参（前端 POST /api/jobs body / 后端 submit 参数）。
#[derive(Debug, Clone, Deserialize)]
pub struct SubmitRequest {
    /// 业务种类。
    pub kind: String,
    /// 业务入参。
    #[serde(default)]
    pub params: serde_json::Value,
    /// 可选标题（缺省由 handler.plan() 生成）。
    #[serde(default)]
    pub title: Option<String>,
    /// 可选优先级。
    #[serde(default)]
    pub priority: Option<i16>,
}

// ───────────────────────── Handler 能力声明（方案 §7.4、§4.5）─────────────────────────

/// 重启模式（方案 §4.5）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Restart {
    /// 不支持重启。
    None,
    /// 从头重跑（幂等 handler：派生新作业）。
    Fresh,
    /// 断点续跑（Resumable handler：跳过已完成项）。
    Resume,
}

/// Handler 能力声明：框架据此决定 UI 按钮可见性与重启策略（方案 §14.3）。
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct JobCaps {
    /// 是否可暂停（handler 在循环埋了 checkpoint）。false → UI 隐藏暂停/停止。
    pub pausable: bool,
    /// 重启模式。
    pub restart: Restart,
    /// 是否幂等（重启/崩溃恢复的前提）。
    pub idempotent: bool,
}

impl Default for JobCaps {
    fn default() -> Self {
        Self {
            pausable: true,
            restart: Restart::Fresh,
            idempotent: true,
        }
    }
}

/// 提交时 handler 预估的执行计划（给进度条基数 + 标题）。
#[derive(Debug, Clone, Default)]
pub struct JobPlan {
    /// 总明细数（进度条基数；0 表示未知）。
    pub total: u64,
    /// 人读标题（缺省时框架用 "kind #id"）。
    pub title: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_terminal_active() {
        assert!(JobStatus::Completed.is_terminal());
        assert!(JobStatus::Cancelled.is_terminal());
        assert!(JobStatus::Failed.is_terminal());
        assert!(!JobStatus::Running.is_terminal());
        assert!(JobStatus::Running.is_active());
        assert!(JobStatus::Paused.is_active());
        assert!(!JobStatus::Pending.is_active());
    }

    #[test]
    fn snapshot_percent() {
        let mut s = ProgressSnapshot {
            total: 42,
            done: 21,
            ..Default::default()
        };
        assert_eq!(s.percent(), 50);
        s.done = 42;
        assert_eq!(s.percent(), 100);
        s.done = 100; // 越界保护
        assert_eq!(s.percent(), 100);
        s.total = 0;
        assert_eq!(s.percent(), 0);
    }

    #[test]
    fn status_serde_snake() {
        let j = serde_json::to_string(&JobStatus::Cancelling).unwrap();
        assert_eq!(j, "\"cancelling\"");
    }
}
