-- =============================================
-- 迁移说明：回滚——删除 M5 分发订阅的三张新表与 md_subscription 扩展列/唯一索引
-- 影响表：md_subscription, md_dispatch_log, md_dist_watermark, md_consumer_offset
-- 操作类型：DROP TABLE / DROP INDEX / DROP COLUMN
-- 回滚方式：无
-- =============================================

DROP TABLE IF EXISTS md_consumer_offset;
DROP TABLE IF EXISTS md_dist_watermark;
DROP TABLE IF EXISTS md_dispatch_log;

DROP INDEX IF EXISTS uk_md_subscription;

ALTER TABLE md_subscription
    DROP COLUMN IF EXISTS name,
    DROP COLUMN IF EXISTS description,
    DROP COLUMN IF EXISTS channel_config,
    DROP COLUMN IF EXISTS event_types,
    DROP COLUMN IF EXISTS retry_max,
    DROP COLUMN IF EXISTS timeout_ms,
    DROP COLUMN IF EXISTS batch_size,
    DROP COLUMN IF EXISTS created_by,
    DROP COLUMN IF EXISTS updated_at;
