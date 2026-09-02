-- 20260902_003 down（手工 DROP；down 迁移按仓内惯例不自动执行）
BEGIN;
SET LOCAL lock_timeout = '5s';
DROP INDEX IF EXISTS idx_cmx_flow_job_acquire;
ALTER TABLE cmx_flow_job DROP COLUMN IF EXISTS claimed_by;
ALTER TABLE cmx_flow_job DROP COLUMN IF EXISTS lease_expires_at;
DROP INDEX IF EXISTS idx_cmx_flow_incident_state;
DROP INDEX IF EXISTS idx_cmx_flow_incident_def;
DROP INDEX IF EXISTS uq_cmx_flow_incident_inst_node;
DROP TABLE IF EXISTS cmx_flow_incident;
COMMIT;
