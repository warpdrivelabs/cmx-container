-- cmx-flow 流程引擎 M5.1：子流程（callActivity）支持
-- 幂等：ALTER TABLE ADD COLUMN IF NOT EXISTS / CREATE INDEX IF NOT EXISTS。
-- 约束遵循：cmx_ 前缀；无 FOREIGN KEY（关联字段 + 索引替代）。
-- 依赖：20260717_001（M1+M2 基础表）。
-- 涉及变更：cmx_flow_instance 补三列，支撑「主流程调用独立子流程并同步等待」。
-- 说明：
--   - callActivity 令牌进入 WAITING_SUBFLOW 态（存于 cmx_flow_token.state，无需改表结构）；
--   - 子流程是独立实例，parent_instance_id 指向主实例、parent_token_id 指向主实例中挂起的令牌；
--   - 子实例完成时按 parent_token_id 精确唤醒主令牌、回写输出变量、继续推进主流程；
--   - org_id 为 M5.2 组织路由预留（M5.1 恒空）。
--   - idx_..._parent 支撑 find_child_instances（按主实例查全部子实例）。

ALTER TABLE cmx_flow_instance ADD COLUMN IF NOT EXISTS org_id             VARCHAR(64);
ALTER TABLE cmx_flow_instance ADD COLUMN IF NOT EXISTS parent_instance_id VARCHAR(64);
ALTER TABLE cmx_flow_instance ADD COLUMN IF NOT EXISTS parent_token_id    VARCHAR(64);
COMMENT ON COLUMN cmx_flow_instance.org_id             IS '所属组织（M5.2 子流程组织路由依据；M5.1 恒空）';
COMMENT ON COLUMN cmx_flow_instance.parent_instance_id IS '父实例 id（子实例指向主实例；主实例为 NULL）';
COMMENT ON COLUMN cmx_flow_instance.parent_token_id    IS '父实例中挂起等待的令牌 id（子完成时据此精确唤醒）';
CREATE INDEX IF NOT EXISTS idx_cmx_flow_instance_parent ON cmx_flow_instance (parent_instance_id);
