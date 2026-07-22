//! DDL —— 任务中心 PG 表结构（幂等，母版 cmx-flow-store-pg::ddl）。
//!
//! 硬约束：表名 `cmx_` 前缀、禁外键、DDL 幂等（IF NOT EXISTS + 补列）。主库 primary。
//! 三张表（方案 §8.1）：
//!   cmx_job            —— 作业主表（状态/进度快照 JSONB/结果/错误）
//!   cmx_job_log        —— 日志/事件流水（M2 预留，活跃期主要走 SSE）
//!   cmx_job_checkpoint —— 断点（M3 断点续跑用，M2 建表占位）

/// 建表 DDL（幂等）。按顺序执行。
pub const DDL_STATEMENTS: &[&str] = &[
    // —— 作业主表 —— //
    r#"CREATE TABLE IF NOT EXISTS cmx_job (
        id            BIGINT       PRIMARY KEY,
        kind          VARCHAR(64)  NOT NULL,
        title         VARCHAR(256) NOT NULL,
        status        VARCHAR(16)  NOT NULL,
        params        JSONB        NOT NULL DEFAULT '{}'::jsonb,
        progress      JSONB,
        result        JSONB,
        error         JSONB,
        priority      SMALLINT     NOT NULL DEFAULT 0,
        origin        VARCHAR(16),
        trigger       VARCHAR(64),
        org_id        BIGINT,
        created_by    BIGINT,
        created_at    BIGINT       NOT NULL,
        started_at    BIGINT,
        finished_at   BIGINT,
        node_id       VARCHAR(64)
    )"#,
    // 幂等补列（既有库升级兜底）。
    "ALTER TABLE cmx_job ADD COLUMN IF NOT EXISTS trigger VARCHAR(64)",
    "ALTER TABLE cmx_job ADD COLUMN IF NOT EXISTS node_id VARCHAR(64)",
    // —— M3 分布式列 —— //
    // heartbeat_at：属主节点周期性刷新的存活心跳（epoch ms）；reaper 据此判属主是否失联。
    "ALTER TABLE cmx_job ADD COLUMN IF NOT EXISTS heartbeat_at BIGINT",
    // control_intent：跨节点控制意图（run/pause/cancel，NULL=无待处理意图）；属主轮询消费。
    "ALTER TABLE cmx_job ADD COLUMN IF NOT EXISTS control_intent VARCHAR(16)",
    // claimed_at：被某节点抢占领取的时刻（epoch ms）。
    "ALTER TABLE cmx_job ADD COLUMN IF NOT EXISTS claimed_at BIGINT",
    // parent_job_id：子作业的父作业 id（M4 子作业 DAG 预留，本轮建列不使用）。
    "ALTER TABLE cmx_job ADD COLUMN IF NOT EXISTS parent_job_id BIGINT",
    "CREATE INDEX IF NOT EXISTS ix_cmx_job_status ON cmx_job (status, created_at DESC)",
    "CREATE INDEX IF NOT EXISTS ix_cmx_job_kind ON cmx_job (kind, status)",
    "CREATE INDEX IF NOT EXISTS ix_cmx_job_org ON cmx_job (org_id, created_at DESC)",
    // 抢占索引：pending 按 (priority DESC, created_at) 出队；属主活跃作业按 node_id+heartbeat 巡检。
    "CREATE INDEX IF NOT EXISTS ix_cmx_job_claim ON cmx_job (status, priority DESC, created_at)",
    "CREATE INDEX IF NOT EXISTS ix_cmx_job_owner ON cmx_job (node_id, status, heartbeat_at)",
    // —— 日志流水 —— //
    r#"CREATE TABLE IF NOT EXISTS cmx_job_log (
        id      BIGINT       PRIMARY KEY,
        job_id  BIGINT       NOT NULL,
        seq     BIGINT       NOT NULL,
        level   VARCHAR(8),
        event   VARCHAR(16),
        text    TEXT,
        data    JSONB,
        at      BIGINT
    )"#,
    "CREATE INDEX IF NOT EXISTS ix_cmx_job_log ON cmx_job_log (job_id, seq)",
    // —— 断点（M3 断点续跑预留）—— //
    r#"CREATE TABLE IF NOT EXISTS cmx_job_checkpoint (
        job_id  BIGINT       PRIMARY KEY,
        cursor  JSONB        NOT NULL,
        rev     BIGINT,
        at      BIGINT
    )"#,
    // —— 历史作业表（RU/HI 分离：删除即归档，转移到历史供审计/查询，与热运行态解耦；
    //    母版 cmx-flow-store-pg cmx_flow_hi_instance）。列与 cmx_job 同构 + archived_at。—— //
    r#"CREATE TABLE IF NOT EXISTS cmx_job_hi (
        id            BIGINT       PRIMARY KEY,
        kind          VARCHAR(64)  NOT NULL,
        title         VARCHAR(256) NOT NULL,
        status        VARCHAR(16)  NOT NULL,
        params        JSONB,
        progress      JSONB,
        result        JSONB,
        error         JSONB,
        priority      SMALLINT     NOT NULL DEFAULT 0,
        origin        VARCHAR(16),
        trigger       VARCHAR(64),
        org_id        BIGINT,
        created_by    BIGINT,
        created_at    BIGINT       NOT NULL,
        started_at    BIGINT,
        finished_at   BIGINT,
        node_id       VARCHAR(64),
        heartbeat_at  BIGINT,
        control_intent VARCHAR(16),
        claimed_at    BIGINT,
        parent_job_id BIGINT,
        archived_at   BIGINT       NOT NULL
    )"#,
    "CREATE INDEX IF NOT EXISTS ix_cmx_job_hi_status ON cmx_job_hi (status, archived_at DESC)",
    "CREATE INDEX IF NOT EXISTS ix_cmx_job_hi_kind ON cmx_job_hi (kind, archived_at DESC)",
    "CREATE INDEX IF NOT EXISTS ix_cmx_job_hi_archived ON cmx_job_hi (archived_at DESC)",
    // —— 历史日志表（作业归档时日志随迁，供事后审计）。—— //
    r#"CREATE TABLE IF NOT EXISTS cmx_job_hi_log (
        id      BIGINT       PRIMARY KEY,
        job_id  BIGINT       NOT NULL,
        seq     BIGINT       NOT NULL,
        level   VARCHAR(8),
        event   VARCHAR(16),
        text    TEXT,
        data    JSONB,
        at      BIGINT
    )"#,
    "CREATE INDEX IF NOT EXISTS ix_cmx_job_hi_log ON cmx_job_hi_log (job_id, seq)",
];

