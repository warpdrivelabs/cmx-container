-- cmx-flow 流程引擎 M4.3：转签家族（转办/加签/委派）
-- 幂等：ALTER ... ADD COLUMN IF NOT EXISTS + CREATE TABLE/INDEX IF NOT EXISTS。
-- 约束遵循：cmx_ 前缀；无 FOREIGN KEY（关联字段 + 索引替代）；标准 TIMESTAMPTZ。
-- 依赖：20260717_001（cmx_flow_task 基础表）。
-- 涉及变更：
--   1) cmx_flow_task 补三列：owner_user_id / parent_task_id / delegation_state（幂等补列，零回归）。
--   2) 新增 cmx_flow_task_delegation（转办/加签/委派流转链台账）。
-- 说明：
--   转办 TRANSFER：assignee+owner 都改新人（彻底换人）。
--   委派 DELEGATE：owner 保持原主，assignee 改代理人，delegation_state=DELEGATED。
--   加签 ADDSIGN_BEFORE/AFTER：原任务挂起(delegation_state=SUSPENDED)，建临时任务(parent_task_id
--     指向原任务，delegation_state=ADDSIGN)，临时办结后原任务恢复。临时任务与原任务共享令牌，
--     流程不推进；可嵌套（临时任务再被加签）。

-- =============================================
-- 1. cmx_flow_task 补列
-- =============================================
ALTER TABLE cmx_flow_task ADD COLUMN IF NOT EXISTS owner_user_id    VARCHAR(64);
ALTER TABLE cmx_flow_task ADD COLUMN IF NOT EXISTS parent_task_id   VARCHAR(64);
ALTER TABLE cmx_flow_task ADD COLUMN IF NOT EXISTS delegation_state VARCHAR(16);
COMMENT ON COLUMN cmx_flow_task.owner_user_id    IS '任务所有者（委派时 ≠ assignee；None=owner即assignee）';
COMMENT ON COLUMN cmx_flow_task.parent_task_id   IS '父任务 id（加签临时任务指向原任务；主任务为 NULL）';
COMMENT ON COLUMN cmx_flow_task.delegation_state IS 'NULL=常规 / DELEGATED / ADDSIGN(临时) / SUSPENDED(被加签挂起)';

-- =============================================
-- 2. 转签台账表
-- =============================================
CREATE TABLE IF NOT EXISTS cmx_flow_task_delegation
(
    id           VARCHAR(64)  NOT NULL,
    task_id      VARCHAR(64)  NOT NULL,
    instance_id  VARCHAR(64)  NOT NULL,
    kind         VARCHAR(20)  NOT NULL,
    from_user_id VARCHAR(64)  NOT NULL,
    to_user_id   VARCHAR(64)  NOT NULL,
    temp_task_id VARCHAR(64),
    reason       VARCHAR(500),
    created_at   TIMESTAMPTZ  NOT NULL,
    PRIMARY KEY (id)
);
COMMENT ON TABLE  cmx_flow_task_delegation              IS '转签台账（转办/加签/委派流转链，供审计与展示）';
COMMENT ON COLUMN cmx_flow_task_delegation.kind         IS 'TRANSFER / ADDSIGN_BEFORE / ADDSIGN_AFTER / DELEGATE';
COMMENT ON COLUMN cmx_flow_task_delegation.temp_task_id IS '加签产生的临时任务 id（转办/委派为 NULL）';
CREATE INDEX IF NOT EXISTS idx_cmx_flow_task_delegation_instance ON cmx_flow_task_delegation (instance_id);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_task_delegation_task     ON cmx_flow_task_delegation (task_id);
