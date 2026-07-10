-- 角色组增加 status 字段（启用/停用），对齐 Role 的 status 语义
ALTER TABLE cmx_role_group ADD COLUMN IF NOT EXISTS status INTEGER NOT NULL DEFAULT 1;

-- 状态索引（性能：按状态过滤角色组树/列表）
CREATE INDEX IF NOT EXISTS idx_cmx_role_group_status ON cmx_role_group(status);
