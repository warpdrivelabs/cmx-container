-- =============================================
-- 迁移说明：回滚 cmx_flow_task_comment 的办理人姓名快照列
-- 影响表：cmx_flow_task_comment
-- 操作类型：DROP COLUMN
-- 回滚方式：无
-- =============================================

ALTER TABLE cmx_flow_task_comment DROP COLUMN IF EXISTS user_name;
ALTER TABLE cmx_flow_task_comment DROP COLUMN IF EXISTS nick_name;
