-- 回滚：cmx-flow 设计器 阶段0 定义持久化层
-- 注意：DROP TABLE 会丢失所有流程定义草稿与版本历史。

DROP TABLE IF EXISTS cmx_flow_definition_version;
DROP TABLE IF EXISTS cmx_flow_definition;
