//! cmx-job-core —— 异步任务中心内核（语义中立，零业务/零 DB 依赖）。
//!
//! 为 `cmx-container` 的长时后端任务（报表计算/校验、凭证记账、销售汇总…）提供统一的
//! 作业生命周期、协作式暂停/停止/重启控制、按作业实时推送详细进度（SSE）。方案见
//! `docs/异步任务中心方案.html`。
//!
//! # 分层角色
//! - 本 crate：模型 + 状态机 + [`JobHandler`]/[`JobContext`] trait + [`JobManager`] + [`JobEventHub`] + 注册表。
//! - `cmx-job-api`：薄 axum handler + SSE 端点 + `JobModule`（web-server 合并）。
//! - 业务 crate（cmx-rpt 等）：实现 [`JobHandler`] + `inventory::submit!` 注册（单向依赖本 crate，无环）。
//!
//! # 启动（对齐 cmx-ai::init_ai_subsystem / cmx-flow 引擎单例）
//! web-server 于 `init_datasources()` 之后调用 [`init_job_subsystem`]，OnceCell 装配全局 [`JobManager`]。
//!
//! # M1 边界
//! 作业状态仅内存（进程重启即丢），暂停占 worker 槽，restart 统一按 Fresh。持久化/断点/分布式留待 M2/M3。

pub mod context;
pub mod event;
pub mod manager;
pub mod model;
pub mod registry;
pub mod runtime;
pub mod store;

pub use context::{Control, JobCancelled, JobContext, JobHandler, LogLevel};
pub use event::{JobEvent, JobEventHub};
pub use manager::{ControlOutcome, JobConfig, JobManager, SUMMARY_CHANNEL};
pub use model::{
    ItemState, Job, JobCaps, JobError, JobId, JobOrigin, JobPlan, JobStatus, ProgressItem,
    ProgressSnapshot, Restart, SubmitRequest,
};
pub use registry::{RegisteredJob, registered_kinds};
pub use store::{JobStore, NullStore};

use std::sync::Arc;
use tokio::sync::OnceCell;

/// 全局 JobManager 单例（handler 层提交/控制/查询/订阅用）。
static MANAGER: OnceCell<JobManager> = OnceCell::const_new();

/// 初始化任务中心子系统（M1 兼容入口：内存态，无持久化）。
///
/// 幂等：多次调用仅首次生效。
pub async fn init_job_subsystem(cfg: JobConfig) {
    init_job_subsystem_with_store(cfg, Arc::new(NullStore)).await;
}

/// 初始化任务中心子系统并注入持久化后端（M2）。
///
/// 启动序列：`ensure_schema`（幂等 DDL）→ 构建 JobManager → `recover`（崩溃恢复残留作业）。
/// schema 不可用时降级为内存态（记 warn，不阻塞 web-server 启动，对齐 flow 引擎容错）。
pub async fn init_job_subsystem_with_store(cfg: JobConfig, store: Arc<dyn JobStore>) {
    init_job_subsystem_full(cfg, store, None).await;
}

/// 初始化任务中心子系统（M3 完整版）：持久化 + 可选终态回调 + 分布式循环。
///
/// - `hook`：终态回调（失败告警等），web-server 注入接 GlobalEventBus。
/// - 分布式模式（cfg.distributed=true）：跳过一次性 recover（改由 reaper 循环持续回收），
///   并启动 claim / heartbeat+reaper / control 三循环。单机模式沿用 M2 的一次性 recover。
pub async fn init_job_subsystem_full(
    cfg: JobConfig,
    store: Arc<dyn JobStore>,
    hook: Option<manager::TerminalHook>,
) {
    if MANAGER.get().is_some() {
        return;
    }
    let schema_ok = match store.ensure_schema().await {
        Ok(()) => true,
        Err(e) => {
            tracing::warn!(error = %e, "任务中心 schema 初始化失败，降级（持久化不可用）");
            false
        }
    };
    let distributed = cfg.distributed;
    let node = cfg.node_id.clone();
    let mut mgr = JobManager::with_store(cfg, store);
    if let Some(h) = hook {
        mgr = mgr.with_terminal_hook(h);
    }
    let kinds = mgr.kinds();
    if schema_ok {
        if distributed {
            // 分布式：不做一次性 recover（reaper 循环会持续回收失联属主的孤儿作业，
            // 且本节点重启前的属主作业由其它节点 reaper 接管——去重靠 SKIP LOCKED）。
            mgr.spawn_distributed_loops();
        } else {
            // 单机：一次性崩溃恢复（幂等重跑 / 非幂等置失败）。
            mgr.recover().await;
        }
    }
    let _ = MANAGER.set(mgr);
    tracing::info!(
        ?kinds,
        persistent = schema_ok,
        distributed,
        node = %node,
        "任务中心子系统已初始化。/api/jobs/* 就绪。"
    );
}

/// 获取全局 [`JobManager`]。返回 `None` 表示尚未 [`init_job_subsystem`]。
pub fn manager() -> Option<&'static JobManager> {
    MANAGER.get()
}

// ───────────────────────── 内置 demo handler（自检 + 端到端冒烟）─────────────────────────

/// 内置演示作业 `job.demo`：跑 N 步，每步 sleep、上报明细/进度、埋 checkpoint。
///
/// 无任何业务/DB 依赖，用于验证提交→进度→暂停/恢复/停止全链路（前端「任务中心」自检按钮）。
/// params: `{ "steps": u64=10, "stepMs": u64=500, "failAt": Option<u64> }`。
pub struct DemoJob;

#[async_trait::async_trait]
impl JobHandler for DemoJob {
    fn kind(&self) -> &'static str {
        "job.demo"
    }

    fn plan(&self, params: &serde_json::Value) -> Result<JobPlan, JobError> {
        let steps = params.get("steps").and_then(|v| v.as_u64()).unwrap_or(10);
        Ok(JobPlan {
            total: steps,
            title: Some(format!("演示作业 · {steps} 步")),
        })
    }

    async fn run(
        &self,
        ctx: &JobContext,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, JobError> {
        let steps = params.get("steps").and_then(|v| v.as_u64()).unwrap_or(10);
        let step_ms = params.get("stepMs").and_then(|v| v.as_u64()).unwrap_or(500);
        let fail_at = params.get("failAt").and_then(|v| v.as_u64());

        ctx.set_phase(1, 1, "演示处理");
        ctx.set_total(steps);
        for i in 1..=steps {
            ctx.checkpoint().await?; // 暂停/停止响应点
            let key = format!("step-{i}");
            ctx.add_item(&key, format!("第 {i} 步"));
            ctx.item_running(&key, "处理中");
            tokio::time::sleep(std::time::Duration::from_millis(step_ms)).await;
            if Some(i) == fail_at {
                ctx.item_fail(&key, "演示失败点");
                ctx.warn(format!("第 {i} 步命中 failAt，标记失败"));
            } else {
                ctx.item_ok(&key, step_ms);
                ctx.info(format!("第 {i}/{steps} 步完成"));
            }
            ctx.progress_inc(1);
        }
        // failWhole=true：整体返回 Err（触发 Failed 终态 + 失败告警回调，供 M3 测试）。
        if params.get("failWhole").and_then(|v| v.as_bool()).unwrap_or(false) {
            return Err(JobError::new(500, "演示整体失败（failWhole）"));
        }
        Ok(serde_json::json!({ "steps": steps, "message": "演示作业完成" }))
    }
}

inventory::submit! { RegisteredJob { make: || Box::new(DemoJob) } }

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn test_mgr() -> JobManager {
        JobManager::new(JobConfig {
            max_concurrency: 4,
            ..Default::default()
        })
    }

    #[tokio::test]
    async fn demo_job_completes() {
        let mgr = test_mgr();
        let id = mgr
            .submit(
                SubmitRequest {
                    kind: "job.demo".into(),
                    params: serde_json::json!({ "steps": 3, "stepMs": 10 }),
                    title: None,
                    priority: None,
                },
                JobOrigin::Backend {
                    trigger: "test".into(),
                },
            )
            .await
            .unwrap();
        // 轮询直至终态。
        for _ in 0..200 {
            if let Some(j) = mgr.get_hot(id) {
                if j.status.is_terminal() {
                    assert_eq!(j.status, JobStatus::Completed);
                    assert_eq!(j.progress.done, 3);
                    assert_eq!(j.progress.ok, 3);
                    return;
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("作业未在预期时间内完成");
    }

    #[tokio::test]
    async fn unknown_kind_rejected() {
        let mgr = test_mgr();
        let r = mgr.submit(
            SubmitRequest {
                kind: "no.such.kind".into(),
                params: serde_json::json!({}),
                title: None,
                priority: None,
            },
            JobOrigin::default(),
        ).await;
        assert!(r.is_err());
    }

    #[tokio::test]
    async fn cancel_running_job() {
        let mgr = test_mgr();
        let id = mgr
            .submit(
                SubmitRequest {
                    kind: "job.demo".into(),
                    params: serde_json::json!({ "steps": 100, "stepMs": 20 }),
                    title: None,
                    priority: None,
                },
                JobOrigin::default(),
            )
            .await
            .unwrap();
        // 等它进 Running。
        tokio::time::sleep(Duration::from_millis(50)).await;
        let out = mgr.cancel(id).await;
        assert!(matches!(out, ControlOutcome::Accepted));
        for _ in 0..200 {
            if let Some(j) = mgr.get_hot(id) {
                if j.status.is_terminal() {
                    assert_eq!(j.status, JobStatus::Cancelled);
                    return;
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("取消未在预期时间内生效");
    }

    #[tokio::test]
    async fn pause_then_resume() {
        let mgr = test_mgr();
        let id = mgr
            .submit(
                SubmitRequest {
                    kind: "job.demo".into(),
                    params: serde_json::json!({ "steps": 20, "stepMs": 30 }),
                    title: None,
                    priority: None,
                },
                JobOrigin::default(),
            )
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(matches!(mgr.pause(id).await, ControlOutcome::Accepted));
        // 等进入 Paused。
        let mut paused = false;
        for _ in 0..100 {
            if mgr.get_hot(id).map(|j| j.status) == Some(JobStatus::Paused) {
                paused = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(paused, "未进入 Paused");
        let done_at_pause = mgr.get_hot(id).unwrap().progress.done;
        // 暂停期间不推进。
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(mgr.get_hot(id).unwrap().progress.done, done_at_pause);
        // 恢复后继续。
        assert!(matches!(mgr.resume(id).await, ControlOutcome::Accepted));
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(mgr.get_hot(id).unwrap().progress.done > done_at_pause);
        mgr.cancel(id).await;
    }
}
