-- 回滚：cmx-flow M5.1 子流程列
-- 注意：DROP COLUMN 会丢失子流程父子关系数据。

DROP INDEX IF EXISTS idx_cmx_flow_instance_parent;
ALTER TABLE cmx_flow_instance DROP COLUMN IF EXISTS parent_token_id;
ALTER TABLE cmx_flow_instance DROP COLUMN IF EXISTS parent_instance_id;
ALTER TABLE cmx_flow_instance DROP COLUMN IF EXISTS org_id;
