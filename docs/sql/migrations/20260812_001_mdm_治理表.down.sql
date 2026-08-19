-- =============================================
-- 迁移说明：回滚——删除 MDM 治理表（反序，与 up 建表顺序相反）
-- 影响表：md_event_log, md_subscription, md_match_scan, md_merge_record, md_match_config, md_value_map, md_xref, md_audit, mdm_activation
-- 操作类型：DROP TABLE
-- 回滚方式：无
-- =============================================

DROP TABLE IF EXISTS md_event_log;
DROP TABLE IF EXISTS md_subscription;
DROP TABLE IF EXISTS md_match_scan;
DROP TABLE IF EXISTS md_merge_record;
DROP TABLE IF EXISTS md_match_config;
DROP TABLE IF EXISTS md_value_map;
DROP TABLE IF EXISTS md_xref;
DROP TABLE IF EXISTS md_audit;
DROP TABLE IF EXISTS mdm_activation;
