-- 回滚：cmx-flow 流程引擎 M3 多实例增量
-- 顺序：先删 mi_scope 表，再删 task 补的列。
-- 注意：DROP TABLE 连带删除其上索引；DROP COLUMN 会丢失该列数据（多实例子任务的元素快照）。

DROP TABLE IF EXISTS cmx_flow_mi_scope;
ALTER TABLE cmx_flow_task DROP COLUMN IF EXISTS element_value;
