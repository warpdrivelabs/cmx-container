-- 回滚：cmx-flow M4.3 转签
-- 注意：DROP COLUMN 会丢失该列数据；DROP TABLE 连带删索引。

DROP TABLE IF EXISTS cmx_flow_task_delegation;
ALTER TABLE cmx_flow_task DROP COLUMN IF EXISTS delegation_state;
ALTER TABLE cmx_flow_task DROP COLUMN IF EXISTS parent_task_id;
ALTER TABLE cmx_flow_task DROP COLUMN IF EXISTS owner_user_id;
