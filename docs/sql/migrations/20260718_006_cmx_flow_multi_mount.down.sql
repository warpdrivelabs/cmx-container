-- 回滚：cmx-flow M5.3 多挂载去重列
-- 注意：DROP COLUMN 会丢失多挂载子实例的发起节点标记，回滚后串行多挂载去重将退化。

ALTER TABLE cmx_flow_instance DROP COLUMN IF EXISTS parent_node_bpmn_id;
