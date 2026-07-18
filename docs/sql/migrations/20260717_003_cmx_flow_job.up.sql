-- cmx-flow 流程引擎 M2.5：定时器作业表（边界定时器）
-- 幂等：CREATE TABLE / INDEX IF NOT EXISTS。
-- 约束遵循：cmx_ 前缀；无 FOREIGN KEY（关联字段 + 索引替代）；标准 TIMESTAMPTZ。
-- 依赖：20260717_001（M1+M2 基础表）、20260717_002（M3 多实例）。
-- 涉及变更：新增 cmx_flow_job（边界定时器「到期待触发」表）。
-- 说明：令牌到达挂有边界定时器的 userTask 时，为每个边界定时器建一条到期作业（记 due_at）。
--       引擎的 trigger_due_timers 在 now >= due_at 时触发：令牌走定时器出边（中断型）或发
--       旁路令牌（非中断型）。随实例聚合快照全删重插，与令牌同生命周期，重启后定时器不丢。
--       idx_..._due 支撑 find_due_jobs 的跨实例到期扫描。

CREATE TABLE IF NOT EXISTS cmx_flow_job
(
    id               VARCHAR(64)  NOT NULL,
    instance_id      VARCHAR(64)  NOT NULL,
    token_id         VARCHAR(64)  NOT NULL,
    boundary_bpmn_id VARCHAR(128) NOT NULL,
    cancel_activity  BOOLEAN      NOT NULL DEFAULT TRUE,
    due_at           TIMESTAMPTZ  NOT NULL,
    created_at       TIMESTAMPTZ  NOT NULL,
    PRIMARY KEY (id)
);
COMMENT ON TABLE  cmx_flow_job                  IS '定时器作业（边界定时器「到期待触发」表）';
COMMENT ON COLUMN cmx_flow_job.instance_id      IS '所属实例 id（逻辑关联 cmx_flow_instance.id）';
COMMENT ON COLUMN cmx_flow_job.token_id         IS '挂载该定时器的令牌 id（停在宿主 userTask）；令牌离开即撤销本作业';
COMMENT ON COLUMN cmx_flow_job.boundary_bpmn_id IS '触发时令牌要去的边界事件节点 bpmn_id';
COMMENT ON COLUMN cmx_flow_job.cancel_activity  IS 'true=中断型(超时中断宿主任务)；false=非中断型(发旁路令牌，宿主不断)';
COMMENT ON COLUMN cmx_flow_job.due_at           IS '到期时刻（宿主到达时刻 + 时长）';
CREATE INDEX IF NOT EXISTS idx_cmx_flow_job_instance ON cmx_flow_job (instance_id);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_job_due      ON cmx_flow_job (due_at);
