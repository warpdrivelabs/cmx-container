-- =============================================
-- 迁移说明：回滚 流程 webhook v2.4 三级路由列
-- 影响表：cmx_flow_instance, cmx_flow_hi_instance, cmx_flow_webhook_delivery
-- 操作类型：DROP INDEX / DROP COLUMN（回滚专用；up 侧禁 DROP，本文件仅手工回滚用）
-- 回滚方式：无（本文件即回滚）
-- =============================================

DROP INDEX IF EXISTS idx_cmx_flow_instance_subscriber;
ALTER TABLE cmx_flow_instance DROP COLUMN IF EXISTS subscriber_id;
ALTER TABLE cmx_flow_hi_instance DROP COLUMN IF EXISTS subscriber_id;
ALTER TABLE cmx_flow_webhook_delivery DROP COLUMN IF EXISTS route_source;
