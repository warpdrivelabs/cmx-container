-- =============================================
-- 迁移说明：回滚 流程引擎出站 webhook 订阅表 + 持久化投递队列表
-- 影响表：cmx_flow_webhook_subscription, cmx_flow_webhook_delivery
-- 操作类型：DROP TABLE / DROP INDEX（回滚专用；up 侧禁 DROP，本文件仅手工回滚用）
-- 回滚方式：无（本文件即回滚）
-- =============================================

DROP INDEX IF EXISTS idx_cmx_flow_webhook_dlv_did;
DROP INDEX IF EXISTS idx_cmx_flow_webhook_dlv_sub;
DROP INDEX IF EXISTS idx_cmx_flow_webhook_dlv_due;
DROP INDEX IF EXISTS uq_cmx_flow_webhook_dlv_sub_event;
DROP INDEX IF EXISTS uq_cmx_flow_webhook_dlv_seq;
DROP TABLE IF EXISTS cmx_flow_webhook_delivery;

DROP INDEX IF EXISTS idx_cmx_flow_webhook_sub_upd;
DROP INDEX IF EXISTS uq_cmx_flow_webhook_sub_name;
DROP TABLE IF EXISTS cmx_flow_webhook_subscription;
