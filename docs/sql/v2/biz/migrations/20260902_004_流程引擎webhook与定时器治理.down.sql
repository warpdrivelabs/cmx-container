-- =============================================
-- 迁移说明：回滚 流程引擎治理批次合并迁移（与 up 四区块反序回滚；按仓内惯例不自动执行）
-- 影响表：cmx_flow_webhook_subscription, cmx_flow_webhook_delivery, cmx_flow_instance,
--         cmx_flow_hi_instance, cmx_flow_job, cmx_flow_incident
-- 操作类型：DROP TABLE / DROP INDEX / DROP COLUMN（回滚专用；up 侧禁 DROP，本文件仅手工回滚用）
-- 回滚方式：无（本文件即回滚）
-- =============================================

-- ═══════════ 回滚 原 20260902_003：定时器租约列 + 故障清单表 ═══════════

DROP INDEX IF EXISTS idx_cmx_flow_job_acquire;
ALTER TABLE cmx_flow_job DROP COLUMN IF EXISTS claimed_by;
ALTER TABLE cmx_flow_job DROP COLUMN IF EXISTS lease_expires_at;
DROP INDEX IF EXISTS idx_cmx_flow_incident_state;
DROP INDEX IF EXISTS idx_cmx_flow_incident_def;
DROP INDEX IF EXISTS uq_cmx_flow_incident_inst_node;
DROP TABLE IF EXISTS cmx_flow_incident;

-- ═══════════ 回滚 原 20260902_002：实例乐观锁与系统归属列 ═══════════

DROP INDEX IF EXISTS idx_cmx_flow_instance_system;
ALTER TABLE cmx_flow_instance    DROP COLUMN IF EXISTS version;
ALTER TABLE cmx_flow_instance    DROP COLUMN IF EXISTS system_id;
ALTER TABLE cmx_flow_hi_instance DROP COLUMN IF EXISTS version;
ALTER TABLE cmx_flow_hi_instance DROP COLUMN IF EXISTS system_id;

-- ═══════════ 回滚 原 20260902_001：三级路由列 ═══════════

DROP INDEX IF EXISTS idx_cmx_flow_instance_subscriber;
ALTER TABLE cmx_flow_instance DROP COLUMN IF EXISTS subscriber_id;
ALTER TABLE cmx_flow_hi_instance DROP COLUMN IF EXISTS subscriber_id;
ALTER TABLE cmx_flow_webhook_delivery DROP COLUMN IF EXISTS route_source;

-- ═══════════ 回滚 原 20260901_001：出站 webhook 订阅表 + 持久化投递队列表 ═══════════

DROP INDEX IF EXISTS idx_cmx_flow_webhook_dlv_did;
DROP INDEX IF EXISTS idx_cmx_flow_webhook_dlv_sub;
DROP INDEX IF EXISTS idx_cmx_flow_webhook_dlv_due;
DROP INDEX IF EXISTS uq_cmx_flow_webhook_dlv_sub_event;
DROP INDEX IF EXISTS uq_cmx_flow_webhook_dlv_seq;
DROP TABLE IF EXISTS cmx_flow_webhook_delivery;

DROP INDEX IF EXISTS idx_cmx_flow_webhook_sub_upd;
DROP INDEX IF EXISTS uq_cmx_flow_webhook_sub_name;
DROP TABLE IF EXISTS cmx_flow_webhook_subscription;
