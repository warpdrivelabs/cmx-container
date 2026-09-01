-- =============================================
-- 迁移说明：流程引擎出站 webhook 订阅表 + 持久化投递队列表（001 方案 v2.3）
-- 影响表：cmx_flow_webhook_subscription, cmx_flow_webhook_delivery
-- 操作类型：CREATE TABLE / CREATE INDEX
-- 回滚方式：20260901_001_流程webhook订阅与投递队列表.down.sql
-- =============================================

-- 订阅配置表（结构对齐 md_subscription；主键 = BIGINT 应用层 Pk52 雪花）
CREATE TABLE IF NOT EXISTS cmx_flow_webhook_subscription (
    id              BIGINT        NOT NULL,
    name            VARCHAR(128)  NOT NULL,
    channel         VARCHAR(16)   NOT NULL DEFAULT 'webhook',
    channel_config  JSONB         NOT NULL DEFAULT '{}',
    definition_keys JSONB         NOT NULL DEFAULT '[]',
    event_types     JSONB         NOT NULL DEFAULT '[]',
    active          BOOLEAN       NOT NULL DEFAULT TRUE,
    retry_max       INT           NOT NULL DEFAULT 10,
    source          VARCHAR(8)    NOT NULL DEFAULT 'manual',
    tenant_id       VARCHAR(64)   NOT NULL DEFAULT 'default',
    created_by      VARCHAR(64),
    created_at      TIMESTAMPTZ   NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ   NOT NULL DEFAULT now(),
    PRIMARY KEY (id)
);
COMMENT ON TABLE  cmx_flow_webhook_subscription              IS '出站 webhook 订阅配置（001 方案；生命周期事件按 definitionKey × 事件类型路由）';
COMMENT ON COLUMN cmx_flow_webhook_subscription.id              IS '主键（应用层 Pk52 雪花，52 位 JS Number 安全）';
COMMENT ON COLUMN cmx_flow_webhook_subscription.name            IS '订阅名（uk：同租户唯一）';
COMMENT ON COLUMN cmx_flow_webhook_subscription.channel         IS '通道类型：webhook（kafka/rabbitmq 预留，save 时校验注册表已注册）';
COMMENT ON COLUMN cmx_flow_webhook_subscription.channel_config  IS '通道配置（开放对象）：webhook {service_key, callback_path, secret——secret 明文，API 掩码回显}；MQ 只存路由目标，连接凭据走 toml';
COMMENT ON COLUMN cmx_flow_webhook_subscription.definition_keys IS '订阅的 definitionKey 集合 JSON 数组；[] = 全部';
COMMENT ON COLUMN cmx_flow_webhook_subscription.event_types     IS '订阅的事件类型集合 JSON 数组；[] = 全部 6 种';
COMMENT ON COLUMN cmx_flow_webhook_subscription.active          IS '启停：停用即不再生成投递行（存量行保留可查可清）';
COMMENT ON COLUMN cmx_flow_webhook_subscription.retry_max       IS '最大尝试次数（含首发，对齐 mdm 口径）；默认 10 = 重试 9 次';
COMMENT ON COLUMN cmx_flow_webhook_subscription.source          IS '来源：env 首启导入 / manual 手工创建';
COMMENT ON COLUMN cmx_flow_webhook_subscription.tenant_id       IS '租户（db-per-tenant 下冗余登记）';
COMMENT ON COLUMN cmx_flow_webhook_subscription.created_by      IS '创建人（M3 死信门户告警的通知对象）';
COMMENT ON COLUMN cmx_flow_webhook_subscription.created_at      IS '创建时间（DB 时钟 now()）';
COMMENT ON COLUMN cmx_flow_webhook_subscription.updated_at      IS '最近更新时间（DB 时钟 now()；缓存 TTL 版本对账轮询列）';
CREATE UNIQUE INDEX IF NOT EXISTS uq_cmx_flow_webhook_sub_name ON cmx_flow_webhook_subscription (tenant_id, name);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_webhook_sub_upd ON cmx_flow_webhook_subscription (updated_at);

-- 持久化投递队列 + 死信一体（事务后持久化队列，非严格 outbox；seq = DB 提交序保序键）
CREATE TABLE IF NOT EXISTS cmx_flow_webhook_delivery (
    id                    BIGINT        NOT NULL,
    seq                   BIGSERIAL     NOT NULL,
    subscription_id       BIGINT        NOT NULL,
    subscription_name     VARCHAR(128)  NOT NULL,
    channel               VARCHAR(16)   NOT NULL,
    event_id              VARCHAR(64)   NOT NULL,
    delivery_id           VARCHAR(160)  NOT NULL,
    source                VARCHAR(8)    NOT NULL DEFAULT 'emit',
    event_type            VARCHAR(32)   NOT NULL,
    definition_key        VARCHAR(128),
    business_key          VARCHAR(128),
    instance_id           VARCHAR(64)   NOT NULL,
    payload               JSONB         NOT NULL,
    state                 VARCHAR(16)   NOT NULL DEFAULT 'PENDING',
    attempts              INT           NOT NULL DEFAULT 0,
    next_attempt_at       TIMESTAMPTZ,
    locked_by             VARCHAR(64),
    lock_expires_at       TIMESTAMPTZ,
    last_error            TEXT,
    last_http_status      INT,
    last_response_snippet VARCHAR(512),
    created_at            TIMESTAMPTZ   NOT NULL DEFAULT now(),
    delivered_at          TIMESTAMPTZ,
    PRIMARY KEY (id)
);
COMMENT ON TABLE  cmx_flow_webhook_delivery              IS '出站事件持久化投递队列（事件×订阅一行；PENDING/IN_FLIGHT/DONE/DEAD/SKIPPED 状态机 + 死信）';
COMMENT ON COLUMN cmx_flow_webhook_delivery.id                    IS '主键（应用层 Pk52 雪花，无排序职责）';
COMMENT ON COLUMN cmx_flow_webhook_delivery.seq                   IS '保序键（BIGSERIAL，DB 按提交序赋值；同订阅按 seq 严格保序的依据，雪花不做保序）';
COMMENT ON COLUMN cmx_flow_webhook_delivery.subscription_id       IS '归属订阅 id → cmx_flow_webhook_subscription.id';
COMMENT ON COLUMN cmx_flow_webhook_delivery.subscription_name     IS '订阅名快照（写入时定版；订阅删除/改名后流水仍可辨识）';
COMMENT ON COLUMN cmx_flow_webhook_delivery.channel               IS '通道快照（写入时定版；多通道后过滤/统计用）';
COMMENT ON COLUMN cmx_flow_webhook_delivery.event_id              IS '事件唯一键（emit 时 uuid；test 行 test-{uuid} 每次唯一）；uk(订阅,事件) 幂等';
COMMENT ON COLUMN cmx_flow_webhook_delivery.delivery_id           IS 'wire 幂等参考键 {instanceId}-{taskId?}-{occurredAt}（x-cmx-flow-delivery 头；索引非唯一）';
COMMENT ON COLUMN cmx_flow_webhook_delivery.source                IS '来源：emit 业务事件 / test 管理页测试（test 直达终态不参与保序）';
COMMENT ON COLUMN cmx_flow_webhook_delivery.event_type            IS '事件类型（instance.started 等 6 种 / webhook.test 伪事件）';
COMMENT ON COLUMN cmx_flow_webhook_delivery.definition_key        IS '流程定义 key（冗余查询列）';
COMMENT ON COLUMN cmx_flow_webhook_delivery.business_key          IS '业务键（冗余查询列）';
COMMENT ON COLUMN cmx_flow_webhook_delivery.instance_id           IS '实例 id（冗余查询列）';
COMMENT ON COLUMN cmx_flow_webhook_delivery.payload               IS '完整事件体 JSONB（投递 body 即此）';
COMMENT ON COLUMN cmx_flow_webhook_delivery.state                 IS '状态：PENDING 待投 / IN_FLIGHT 投递中 / DONE 成功 / DEAD 死信 / SKIPPED 人工处置；终态均不阻塞同订阅保序';
COMMENT ON COLUMN cmx_flow_webhook_delivery.attempts              IS '已尝试次数（claim 时 +1；retry_max 含首发）';
COMMENT ON COLUMN cmx_flow_webhook_delivery.next_attempt_at       IS '退避到期时间（1s 起指数封顶 5min）';
COMMENT ON COLUMN cmx_flow_webhook_delivery.locked_by             IS '租约持有者（worker id；多副本投递互斥）';
COMMENT ON COLUMN cmx_flow_webhook_delivery.lock_expires_at       IS '租约到期（120s；逐行续租，过期可被重抢自愈）';
COMMENT ON COLUMN cmx_flow_webhook_delivery.last_error            IS '最近失败原因（死信诊断）';
COMMENT ON COLUMN cmx_flow_webhook_delivery.last_http_status      IS '最近 HTTP 状态码（死信诊断）';
COMMENT ON COLUMN cmx_flow_webhook_delivery.last_response_snippet IS '响应摘要（截断 512，死信诊断）';
COMMENT ON COLUMN cmx_flow_webhook_delivery.created_at            IS '创建时间';
COMMENT ON COLUMN cmx_flow_webhook_delivery.delivered_at          IS '投递成功时间';
CREATE UNIQUE INDEX IF NOT EXISTS uq_cmx_flow_webhook_dlv_seq       ON cmx_flow_webhook_delivery (seq);
CREATE UNIQUE INDEX IF NOT EXISTS uq_cmx_flow_webhook_dlv_sub_event ON cmx_flow_webhook_delivery (subscription_id, event_id);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_webhook_dlv_due ON cmx_flow_webhook_delivery (state, next_attempt_at);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_webhook_dlv_sub ON cmx_flow_webhook_delivery (subscription_id, seq);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_webhook_dlv_did ON cmx_flow_webhook_delivery (delivery_id);
