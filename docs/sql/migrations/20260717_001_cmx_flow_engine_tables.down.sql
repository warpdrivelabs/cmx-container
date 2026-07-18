-- 回滚：删除 cmx-flow 流程引擎的运行态与历史态表
-- 顺序：先历史表后运行态表（无外键，顺序其实无关，但保持逻辑清晰）。
-- 注意：DROP TABLE 会连带删除其上的索引，无需单独 DROP INDEX。

DROP TABLE IF EXISTS cmx_flow_hi_task;
DROP TABLE IF EXISTS cmx_flow_hi_instance;
DROP TABLE IF EXISTS cmx_flow_task;
DROP TABLE IF EXISTS cmx_flow_token;
DROP TABLE IF EXISTS cmx_flow_instance;
