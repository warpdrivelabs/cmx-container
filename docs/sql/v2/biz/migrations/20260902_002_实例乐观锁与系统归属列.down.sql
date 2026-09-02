-- 20260902_002 down（手工 DROP；down 迁移按仓内惯例不自动执行）
BEGIN;
SET LOCAL lock_timeout = '5s';
DROP INDEX IF EXISTS idx_cmx_flow_instance_system;
ALTER TABLE cmx_flow_instance    DROP COLUMN IF EXISTS version;
ALTER TABLE cmx_flow_instance    DROP COLUMN IF EXISTS system_id;
ALTER TABLE cmx_flow_hi_instance DROP COLUMN IF EXISTS version;
ALTER TABLE cmx_flow_hi_instance DROP COLUMN IF EXISTS system_id;
COMMIT;
