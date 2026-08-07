//! 异步任务中心（Job Center）初始化
//!
//! M3 分布式态：注入 PG 持久化后端（`cmx_job_*` 三表，主库 primary）+ 终态回调
//! （失败告警 → `GlobalEventBus`）。`distributed=true` 时启动 claim/heartbeat + reaper/control
//! 三循环：多实例经 `UPDATE...SKIP LOCKED` 抢占 pending 作业本地执行（不重跑），失联属主由
//! reaper 回收。

use tracing::warn;

/// 初始化异步任务中心。
///
/// `node_id` 取环境变量 `CMX_JOB_NODE_ID`（多实例须各异）；未设则用 `"node-<pid>"`。
/// `distributed` 取 `JOB_DISTRIBUTED`（默认 `true`；单机也安全——一个节点独占抢占）。
/// 终态回调：失败作业发 `GlobalEventBus`（`job.failed`），供告警/通知消费者订阅。
pub async fn init_job_center() {
    let node_id = std::env::var("CMX_JOB_NODE_ID")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| format!("node-{}", std::process::id()));
    let distributed = std::env::var("JOB_DISTRIBUTED")
        .map(|v| v != "false" && v != "0")
        .unwrap_or(true);
    let job_cfg = cmx_job_core::JobConfig {
        max_concurrency: 4,
        distributed,
        node_id,
        owner_timeout_ms: 30_000,
    };
    // 终态回调：失败作业发 GlobalEventBus（job.failed），供告警/通知消费者订阅。
    let hook: cmx_job_core::manager::TerminalHook = std::sync::Arc::new(|job: &cmx_job_core::Job| {
        if job.status == cmx_job_core::JobStatus::Failed {
            let jid = job.id;
            let payload = serde_json::json!({
                "id": job.id.to_string(),
                "kind": job.kind,
                "title": job.title,
                "error": job.error,
            });
            warn!(job_id = jid, "作业失败告警 → GlobalEventBus(job.failed)");
            tokio::spawn(async move {
                if cmx_traits::event_bus::GlobalEventBus::is_initialized() {
                    cmx_traits::event_bus::GlobalEventBus::get()
                        .publish("job.failed", payload)
                        .await;
                }
            });
        }
    });
    cmx_job_core::init_job_subsystem_full(
        job_cfg,
        std::sync::Arc::new(cmx_job_store_pg::PgJobStore::default_db()),
        Some(hook),
    )
    .await;
}
