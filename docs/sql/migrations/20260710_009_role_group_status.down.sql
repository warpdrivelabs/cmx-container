-- 回滚：移除角色组 status 字段
DROP INDEX IF EXISTS idx_cmx_role_group_status;
ALTER TABLE cmx_role_group DROP COLUMN IF EXISTS status;
