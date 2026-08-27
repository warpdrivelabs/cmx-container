-- =============================================
-- 迁移说明：cmx_flow_task_comment 新增办理人姓名快照列 user_name / nick_name，
--           办结/退回写意见时随行落快照（写入时点定版）——人员后续改名/删号
--           不影响历史审批记录展示；仅新数据有值，存量留 NULL（如需补刷按
--           user_id 关联 cmx_user 手工回填，引擎不做数据迁移）。
-- 影响表：cmx_flow_task_comment
-- 操作类型：ADD COLUMN
-- 回滚方式：20260827_001_流程意见加办理人姓名快照列.down.sql
-- =============================================

ALTER TABLE cmx_flow_task_comment ADD COLUMN IF NOT EXISTS user_name VARCHAR(128);
COMMENT ON COLUMN cmx_flow_task_comment.user_name IS '办理人用户名快照（写入时点 username 口径展示名）';

ALTER TABLE cmx_flow_task_comment ADD COLUMN IF NOT EXISTS nick_name VARCHAR(128);
COMMENT ON COLUMN cmx_flow_task_comment.nick_name IS '办理人昵称快照（写入时点 nickname 优先、username 兜底）';
