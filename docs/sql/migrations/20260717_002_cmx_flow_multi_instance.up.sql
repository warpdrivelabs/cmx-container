-- cmx-flow 流程引擎 M3：多实例（会签/或签）表结构增量
-- 幂等：CREATE TABLE / INDEX IF NOT EXISTS；ALTER TABLE ADD COLUMN IF NOT EXISTS。
-- 约束遵循：cmx_ 前缀；无 FOREIGN KEY（关联字段 + 索引替代）；标准 TIMESTAMPTZ。
-- 依赖：20260717_001_cmx_flow_engine_tables.up.sql（M1+M2 基础表）。
-- 涉及变更：
--   1) cmx_flow_task 补列 element_value JSONB（会签每个子任务携带各自的当前元素）。
--   2) 新增 cmx_flow_mi_scope（多实例执行域：会签/或签的计数与游标账本）。
-- 说明：多实例把「一个逻辑 userTask」展开成「多个并发/顺序子任务」，需要一个域记账
--       （总数/已完成/顺序游标/完成条件）。它随实例聚合快照全删重插，与令牌/任务同生命周期。

-- =============================================
-- 1. cmx_flow_task 补列：element_value
-- =============================================
ALTER TABLE cmx_flow_task ADD COLUMN IF NOT EXISTS element_value JSONB;
COMMENT ON COLUMN cmx_flow_task.element_value IS '多实例子任务携带的当前元素（会签每人各自的数据；单实例任务为 NULL）';

-- =============================================
-- 2. 多实例执行域表（会签/或签账本）
-- =============================================
CREATE TABLE IF NOT EXISTS cmx_flow_mi_scope
(
    id                   VARCHAR(64)  NOT NULL,
    instance_id          VARCHAR(64)  NOT NULL,
    node_bpmn_id         VARCHAR(128) NOT NULL,
    sequential           BOOLEAN      NOT NULL DEFAULT FALSE,
    total                INTEGER      NOT NULL,
    completed            INTEGER      NOT NULL DEFAULT 0,
    next_index           INTEGER      NOT NULL DEFAULT 0,
    collection           JSONB        NOT NULL DEFAULT '[]'::jsonb,
    element_var          VARCHAR(128),
    completion_condition VARCHAR(512),
    finished             BOOLEAN      NOT NULL DEFAULT FALSE,
    PRIMARY KEY (id)
);
COMMENT ON TABLE  cmx_flow_mi_scope                      IS '多实例执行域（会签/或签的一次展开：计数与游标账本）';
COMMENT ON COLUMN cmx_flow_mi_scope.instance_id          IS '所属实例 id（逻辑关联 cmx_flow_instance.id）';
COMMENT ON COLUMN cmx_flow_mi_scope.node_bpmn_id         IS '对应 multiInstance 节点的 BPMN id';
COMMENT ON COLUMN cmx_flow_mi_scope.sequential           IS 'true=顺序(或签，逐个办理)；false=并行(会签，齐头并进)';
COMMENT ON COLUMN cmx_flow_mi_scope.total                IS '展开的子实例总数（nrOfInstances）';
COMMENT ON COLUMN cmx_flow_mi_scope.completed            IS '已办结的子实例数（nrOfCompletedInstances）';
COMMENT ON COLUMN cmx_flow_mi_scope.next_index           IS '顺序模式下一个待展开元素下标；并行模式恒等于 total';
COMMENT ON COLUMN cmx_flow_mi_scope.collection           IS '展开用的元素快照（JSONB 数组，定格避免中途变量被改）';
COMMENT ON COLUMN cmx_flow_mi_scope.element_var          IS '子任务携带当前元素的变量名（elementVariable，可空）';
COMMENT ON COLUMN cmx_flow_mi_scope.completion_condition IS '完成条件表达式（可空；命中即提前收口剩余子实例）';
COMMENT ON COLUMN cmx_flow_mi_scope.finished             IS '本域是否已收口（完成条件命中或自然全部完成）';
CREATE INDEX IF NOT EXISTS idx_cmx_flow_mi_scope_instance ON cmx_flow_mi_scope (instance_id);
