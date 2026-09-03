-- =============================================
-- 迁移说明：回滚 流程引擎事件订阅分组与实例治理（与 up 区块反序回滚；按仓内惯例不自动执行）。
--           注意：v2.3/v2.4 旧 webhook 设计（cmx_flow_webhook_* 两表、subscriber_id 列）
--           已在 up 侧拆除且**不恢复**——本文件只回滚新建结构（分组/订阅者/投递三表、
--           定义 group_id、乐观锁/系统归属、定时器租约、故障清单）。
-- 影响表：cmx_flow_event_delivery, cmx_flow_event_subscriber, cmx_flow_def_group,
--         cmx_flow_definition, cmx_flow_instance, cmx_flow_hi_instance,
--         cmx_flow_job, cmx_flow_incident
-- 操作类型：DROP TABLE / DROP INDEX / DROP COLUMN（回滚专用；up 侧除豁免清理段外禁 DROP）
-- 回滚方式：无（本文件即回滚）
-- =============================================

-- ═══════════ 回滚 六：定时器租约列 + 故障清单表 ═══════════

DROP INDEX IF EXISTS idx_cmx_flow_job_acquire;
ALTER TABLE cmx_flow_job DROP COLUMN IF EXISTS claimed_by;
ALTER TABLE cmx_flow_job DROP COLUMN IF EXISTS lease_expires_at;
DROP INDEX IF EXISTS idx_cmx_flow_incident_state;
DROP INDEX IF EXISTS idx_cmx_flow_incident_def;
DROP INDEX IF EXISTS uq_cmx_flow_incident_inst_node;
DROP TABLE IF EXISTS cmx_flow_incident;

-- ═══════════ 回滚 五：实例乐观锁与系统归属列 ═══════════

DROP INDEX IF EXISTS idx_cmx_flow_instance_system;
ALTER TABLE cmx_flow_instance    DROP COLUMN IF EXISTS version;
ALTER TABLE cmx_flow_instance    DROP COLUMN IF EXISTS system_id;
ALTER TABLE cmx_flow_hi_instance DROP COLUMN IF EXISTS version;
ALTER TABLE cmx_flow_hi_instance DROP COLUMN IF EXISTS system_id;

-- ═══════════ 回滚 四：事件投递表 ═══════════

DROP INDEX IF EXISTS idx_cmx_flow_event_dlv_created;
DROP INDEX IF EXISTS idx_cmx_flow_event_dlv_did;
DROP INDEX IF EXISTS idx_cmx_flow_event_dlv_sub;
DROP INDEX IF EXISTS idx_cmx_flow_event_dlv_due;
DROP INDEX IF EXISTS uq_cmx_flow_event_dlv_sub_event;
DROP INDEX IF EXISTS uq_cmx_flow_event_dlv_seq;
DROP TABLE IF EXISTS cmx_flow_event_delivery;

-- ═══════════ 回滚 三：事件订阅者表 ═══════════

DROP INDEX IF EXISTS idx_cmx_flow_event_sub_upd;
DROP INDEX IF EXISTS uq_cmx_flow_event_sub_name;
DROP TABLE IF EXISTS cmx_flow_event_subscriber;

-- ═══════════ 回滚 二：流程分组表 + 定义分组列 ═══════════

DROP INDEX IF EXISTS idx_cmx_flow_definition_group;
ALTER TABLE cmx_flow_definition DROP COLUMN IF EXISTS group_id;
DROP INDEX IF EXISTS uq_cmx_flow_def_group_name;
DROP TABLE IF EXISTS cmx_flow_def_group;
