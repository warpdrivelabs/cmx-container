-- 回滚 note 列。
ALTER TABLE cmx_flow_definition_version DROP COLUMN IF EXISTS note;
