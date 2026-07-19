-- 回滚 DAM 三段列（module 属 007，不回滚）。
DROP INDEX IF EXISTS idx_cmx_flow_definition_dam;
ALTER TABLE cmx_flow_definition DROP COLUMN IF EXISTS application;
ALTER TABLE cmx_flow_definition DROP COLUMN IF EXISTS domain;
