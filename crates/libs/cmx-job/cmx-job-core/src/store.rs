//! [`JobStore`] —— 作业持久化抽象（M2 持久化的接缝）。
//!
//! [`JobManager`](crate::JobManager) 在关键点（提交/状态跃迁/进度去抖/终态）经此 trait 写穿到后端。
//! `cmx-job-core` 只定义 trait 与一个零成本的 [`NullStore`]（M1 内存态默认）；PG 实现在
//! `cmx-job-store-pg`（core 不依赖 DB，破环方向与 ReportModule 一致）。
//!
//! 设计要点（方案 §8）：
//!   - **省写**：状态跃迁必落（低频、必准）；进度快照去抖落（阶段切换/完成/失败/每 N 秒）。
//!   - **不阻塞 handler**：写库失败只告警不 panic（进度是内存权威，DB 是备份，方案 §14.1）。
//!   - **崩溃恢复**：启动时 [`JobStore::load_active`] 读出残留非终态作业，由 Manager 判幂等后重入队/置失败。

use async_trait::async_trait;

use crate::model::{Job, JobStatus};

/// 作业持久化后端。所有写方法幂等/容错：实现应吞掉可恢复错误（记 warn），不向上 panic。
#[async_trait]
pub trait JobStore: Send + Sync {
    /// 启动期确保表结构就绪（幂等 DDL）。返回 Err 表示 schema 不可用（调用方降级为内存态）。
    async fn ensure_schema(&self) -> Result<(), String>;

    /// 插入一条新作业（提交时）。
    async fn insert(&self, job: &Job);

    /// 更新作业状态 + 时间戳 + 进度（状态跃迁时，必落）。
    async fn update_status(&self, job: &Job);

    /// 更新进度快照（去抖后调用，非每次）。
    async fn update_progress(&self, job: &Job);

    /// 落终态（状态 + result + error + finished_at + 最终进度）。
    async fn finish(&self, job: &Job);

    /// 追加一条日志（M2 简化为直插；未来可异步批量）。
    async fn append_log(&self, job_id: i64, seq: i64, level: &str, event: &str, text: &str, at: i64);

    /// 归档作业（删除语义调整为 RU/HI 分离）：把作业行 + 日志从活跃表原子转移到历史表
    /// （`cmx_job`→`cmx_job_hi`、`cmx_job_log`→`cmx_job_hi_log`），再从活跃表删除。
    /// 非真删——数据与最终状态保留在历史表供审计/查询。母版 cmx-flow RU/HI。
    async fn archive(&self, job_id: i64);

    /// 读取历史作业（按 kind/status 过滤，archived_at 倒序，分页 offset/limit）。
    async fn list_history(
        &self,
        _kind: Option<&str>,
        _status: Option<JobStatus>,
        _offset: usize,
        _limit: usize,
    ) -> Vec<Job> {
        Vec::new()
    }

    /// 读取单条历史作业。
    async fn get_history(&self, _job_id: i64) -> Option<Job> {
        None
    }

    /// 读取历史作业总数（与 list_history 同过滤条件，保证 total 与 items 一致）。
    async fn count_history(&self, _kind: Option<&str>, _status: Option<JobStatus>) -> u64 {
        0
    }

    /// 读取活跃/历史合并前的持久化作业列表（按 kind/status 过滤，倒序，limit）。用于进程重启后列表展示。
    async fn list(&self, kind: Option<&str>, status: Option<JobStatus>, limit: usize) -> Vec<Job>;

    /// 读取单作业（内存表未命中时的兜底）。
    async fn get(&self, job_id: i64) -> Option<Job>;

    /// 崩溃恢复第一步：读出所有非终态残留作业（pending/running/paused/cancelling）。
    ///
    /// Manager 拿到后按 handler 幂等能力判裁决（幂等→重跑，非幂等→置失败），再各自落库。
    /// 本方法只读，不改状态。
    async fn load_active(&self) -> Vec<Job>;

    // ───────────────────────── M3 分布式（表驱动抢占）─────────────────────────

    /// 原子抢占一批 pending 作业归本节点执行（`UPDATE...FOR UPDATE SKIP LOCKED RETURNING`）。
    ///
    /// 把 `status='pending'` 的行按 `(priority DESC, created_at)` 取最多 `limit` 条，原子置为
    /// `running` + `node_id`+`claimed_at`+`heartbeat_at`，返回被本节点领到的作业。多实例并发调用
    /// 各领不相交子集（SKIP LOCKED 保证不重领），是分布式不重跑的核心。默认实现返回空（单机 NullStore）。
    async fn claim_pending(&self, _node_id: &str, _limit: usize, _now: i64) -> Vec<Job> {
        Vec::new()
    }

    /// 刷新本节点属主作业的心跳（`heartbeat_at=now`），供 reaper 判活。默认 no-op。
    async fn heartbeat(&self, _node_id: &str, _job_ids: &[i64], _now: i64) {}

    /// 回收失联属主的活跃作业：把 `heartbeat_at < now-timeout` 的 running/paused/cancelling
    /// 作业重置为 `pending`（清 node_id），供其它节点重领。返回被回收的作业 id。默认空。
    async fn reap_dead_owners(&self, _timeout_ms: i64, _now: i64) -> Vec<i64> {
        Vec::new()
    }

    /// 写入跨节点控制意图（pause/resume/cancel）到作业行的 control_intent 列。默认 no-op。
    async fn set_control_intent(&self, _job_id: i64, _intent: &str) {}

    /// 读取并清除本节点属主作业的待处理控制意图（`{job_id → intent}`）。默认空。
    async fn take_control_intents(&self, _node_id: &str) -> Vec<(i64, String)> {
        Vec::new()
    }
}

/// 零持久化实现（M1 内存态默认；所有写操作 no-op）。
#[derive(Default)]
pub struct NullStore;

#[async_trait]
impl JobStore for NullStore {
    async fn ensure_schema(&self) -> Result<(), String> {
        Ok(())
    }
    async fn insert(&self, _job: &Job) {}
    async fn update_status(&self, _job: &Job) {}
    async fn update_progress(&self, _job: &Job) {}
    async fn finish(&self, _job: &Job) {}
    async fn append_log(&self, _id: i64, _seq: i64, _l: &str, _e: &str, _t: &str, _at: i64) {}
    async fn archive(&self, _job_id: i64) {}
    async fn list(&self, _k: Option<&str>, _s: Option<JobStatus>, _n: usize) -> Vec<Job> {
        Vec::new()
    }
    async fn get(&self, _job_id: i64) -> Option<Job> {
        None
    }
    async fn load_active(&self) -> Vec<Job> {
        Vec::new()
    }
}

