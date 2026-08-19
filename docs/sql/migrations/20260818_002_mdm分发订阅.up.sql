-- =============================================
-- 迁移说明：M5 分发订阅——md_subscription 扩展（通道配置/事件类型/重试策略）+ 新建投递实例表 md_dispatch_log、扇出水位表 md_dist_watermark、pull 游标登记表 md_consumer_offset
-- 影响表：md_subscription, md_dispatch_log, md_dist_watermark, md_consumer_offset
-- 操作类型：ADD COLUMN / CREATE TABLE / CREATE INDEX / INSERT (seed)
-- 回滚方式：20260818_002_mdm分发订阅.down.sql
-- =============================================

-- ─────────────────────────────────────────────
-- 区块 1：md_subscription 扩展（既有 7 列保留兼容）
-- ─────────────────────────────────────────────
ALTER TABLE md_subscription
    ADD COLUMN IF NOT EXISTS name           VARCHAR(128),
    ADD COLUMN IF NOT EXISTS description    VARCHAR(512),
    ADD COLUMN IF NOT EXISTS channel_config JSONB        NOT NULL DEFAULT '{}',
    ADD COLUMN IF NOT EXISTS event_types    JSONB        NOT NULL DEFAULT '[]',
    ADD COLUMN IF NOT EXISTS retry_max      INT          NOT NULL DEFAULT 8,
    ADD COLUMN IF NOT EXISTS timeout_ms     INT          NOT NULL DEFAULT 10000,
    ADD COLUMN IF NOT EXISTS batch_size     INT          NOT NULL DEFAULT 50,
    ADD COLUMN IF NOT EXISTS created_by     VARCHAR(64),
    ADD COLUMN IF NOT EXISTS updated_at     TIMESTAMPTZ  NOT NULL DEFAULT now();

COMMENT ON COLUMN md_subscription.name           IS '订阅名称（展示）';
COMMENT ON COLUMN md_subscription.channel_config IS '通道配置：webhook {url,secret,headers{}}；rest_pull {consumerId}；kafka {brokers,topic,partition_key}（骨架）';
COMMENT ON COLUMN md_subscription.event_types    IS '订阅事件类型 JSON 数组；[] = 全部(created/updated/merged)';
COMMENT ON COLUMN md_subscription.retry_max     IS '最大尝试次数（含首发）';
COMMENT ON COLUMN md_subscription.timeout_ms    IS '单次投递超时（毫秒）';
COMMENT ON COLUMN md_subscription.batch_size    IS '单轮该订阅最大投递数';
COMMENT ON COLUMN md_subscription.created_by    IS '创建人用户 id';
COMMENT ON COLUMN md_subscription.updated_at    IS '最近更新时间';
COMMENT ON COLUMN md_subscription.channel       IS '通道 webhook/kafka/rocketmq/rest_pull';

CREATE UNIQUE INDEX IF NOT EXISTS uk_md_subscription ON md_subscription (target_sys, dict_code, channel);

-- ─────────────────────────────────────────────
-- 区块 2：md_dispatch_log 投递实例表（事件×订阅：队列状态机 + 投递流水）
-- ─────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS md_dispatch_log (
    id               BIGINT       NOT NULL,
    subscription_id  BIGINT       NOT NULL,
    event_id         VARCHAR(64)  NOT NULL,
    event_seq        BIGINT       NOT NULL,
    dict_code        VARCHAR(64)  NOT NULL,
    record_id        BIGINT       NOT NULL,
    status           VARCHAR(16)  NOT NULL,
    attempts         INT          NOT NULL DEFAULT 0,
    next_retry_at    TIMESTAMPTZ,
    last_error       TEXT,
    http_status      INT,
    response_snippet VARCHAR(512),
    delivered_at     TIMESTAMPTZ,
    created_at       TIMESTAMPTZ  NOT NULL DEFAULT now(),
    updated_at       TIMESTAMPTZ  NOT NULL DEFAULT now(),
    PRIMARY KEY (id)
);

COMMENT ON TABLE  md_dispatch_log IS '分发投递实例（事件×订阅）：队列状态机 + 投递流水';
COMMENT ON COLUMN md_dispatch_log.id IS '主键（应用层 snowflake，对齐 md_ 治理表惯例）';
COMMENT ON COLUMN md_dispatch_log.subscription_id IS '订阅 id → md_subscription.id';
COMMENT ON COLUMN md_dispatch_log.event_id IS '事件 id → md_event_log.id（幂等键之一）';
COMMENT ON COLUMN md_dispatch_log.event_seq IS '事件序号 → md_event_log.seq（排序/诊断冗余）';
COMMENT ON COLUMN md_dispatch_log.dict_code IS '字典代码（冗余，过滤用）';
COMMENT ON COLUMN md_dispatch_log.record_id IS '主数据记录 id';
COMMENT ON COLUMN md_dispatch_log.status IS 'pending待投/running投递中/delivered成功/failed待重试/dead死信/skipped人工跳过';
COMMENT ON COLUMN md_dispatch_log.attempts IS '已尝试次数';
COMMENT ON COLUMN md_dispatch_log.next_retry_at IS 'failed 的下次可抢占时间（指数退避）；NULL=非 failed';
COMMENT ON COLUMN md_dispatch_log.last_error IS '最近一次错误信息';
COMMENT ON COLUMN md_dispatch_log.http_status IS 'webhook 响应码';
COMMENT ON COLUMN md_dispatch_log.response_snippet IS '响应体摘要（截断 512）';
COMMENT ON COLUMN md_dispatch_log.delivered_at IS '投递成功时间';
COMMENT ON COLUMN md_dispatch_log.created_at IS '创建时间';
COMMENT ON COLUMN md_dispatch_log.updated_at IS '最近状态变更时间';

CREATE UNIQUE INDEX IF NOT EXISTS uk_md_dispatch_sub_event ON md_dispatch_log (subscription_id, event_id);
CREATE INDEX IF NOT EXISTS idx_md_dispatch_due   ON md_dispatch_log (status, next_retry_at);
CREATE INDEX IF NOT EXISTS idx_md_dispatch_sub   ON md_dispatch_log (subscription_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_md_dispatch_event ON md_dispatch_log (event_id);

-- ─────────────────────────────────────────────
-- 区块 3：md_dist_watermark 扇出水位表（全局单行 fanout）
-- ─────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS md_dist_watermark (
    key        VARCHAR(32) NOT NULL,
    last_seq   BIGINT      NOT NULL DEFAULT 0,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (key)
);

COMMENT ON TABLE  md_dist_watermark IS '分发引擎扇出水位（全局单行 fanout）';
COMMENT ON COLUMN md_dist_watermark.key IS '水位键（当前仅 fanout）';
COMMENT ON COLUMN md_dist_watermark.last_seq IS '已扇出处理的 md_event_log 最大 seq（无论是否命中订阅）';
COMMENT ON COLUMN md_dist_watermark.updated_at IS '最近推进时间';

INSERT INTO md_dist_watermark (key, last_seq) VALUES ('fanout', 0) ON CONFLICT (key) DO NOTHING;

-- ─────────────────────────────────────────────
-- 区块 4：md_consumer_offset pull 游标登记表（监控/对账用）
-- ─────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS md_consumer_offset (
    id          BIGINT      NOT NULL,
    consumer_id VARCHAR(64) NOT NULL,
    dict_code   VARCHAR(64) NOT NULL,
    acked_seq   BIGINT      NOT NULL DEFAULT 0,
    acked_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (id)
);

COMMENT ON TABLE  md_consumer_offset IS 'pull 消费者游标登记（监控/对账用；消费端仍应自持 seq）';
COMMENT ON COLUMN md_consumer_offset.id IS '主键（应用层 snowflake）';
COMMENT ON COLUMN md_consumer_offset.consumer_id IS '下游消费者标识（建议 = target_sys）';
COMMENT ON COLUMN md_consumer_offset.dict_code IS '字典代码';
COMMENT ON COLUMN md_consumer_offset.acked_seq IS '已确认消费到的 seq';
COMMENT ON COLUMN md_consumer_offset.acked_at IS '最近确认时间';

CREATE UNIQUE INDEX IF NOT EXISTS uk_md_consumer_offset ON md_consumer_offset (consumer_id, dict_code);
