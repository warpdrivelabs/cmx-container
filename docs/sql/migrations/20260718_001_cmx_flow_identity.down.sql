-- 回滚：cmx-flow M4.1 组织/岗位/候选人池
-- 注意：DROP TABLE 连带删除索引。cmx_org/cmx_position 若已被 cmx_user.org_id 等引用，
--       回滚前需确认无依赖数据。

DROP TABLE IF EXISTS cmx_flow_task_candidate;
DROP TABLE IF EXISTS cmx_user_position;
DROP TABLE IF EXISTS cmx_position;
DROP TABLE IF EXISTS cmx_org;
