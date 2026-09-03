-- =============================================
-- 迁移说明：流程引擎治理批次合并迁移（重写版，2026-09-02 推倒重构拍板）。
--           本文件由原「20260902_004_流程引擎webhook与定时器治理」原地改写并改号为本 005：
--           ① 拆除 v2.3/v2.4 webhook 旧设计（订阅/投递两表 + 三级路由 subscriber_id 列）——
--              旧表当日建、未投产、无生产存量，经用户明示豁免 DROP 红线，幂等清理段收口
--              （已执行过旧版 004 的库重放无副作用；fresh 库零动作）；
--           ② 新建事件订阅三表：流程分组表 + 定义分组列、事件订阅者表（rules JSONB 内嵌）、
--              事件投递表（方案见 documents/plans/20260902_流程引擎_事件订阅与流程分组重构方案.md）；
--           ③ 保留原批次的技术债治理段：实例乐观锁与系统归属列、定时器租约列与故障清单表
--              （原 20260902_002/003 内容，逐字节不动）。
-- 影响表：cmx_flow_webhook_subscription(拆), cmx_flow_webhook_delivery(拆),
--         cmx_flow_instance, cmx_flow_hi_instance, cmx_flow_def_group(新),
--         cmx_flow_definition, cmx_flow_event_subscriber(新), cmx_flow_event_delivery(新),
--         cmx_flow_job, cmx_flow_incident(新)
-- 操作类型：DROP TABLE IF EXISTS / DROP COLUMN IF EXISTS / CREATE TABLE / ADD COLUMN /
--           CREATE INDEX / COMMENT
-- 回滚方式：20260902_005_流程引擎事件订阅分组与实例治理.down.sql（旧 webhook 设计不恢复）
-- =============================================

-- ═══════════ 一、旧 webhook 设计拆除（幂等清理段；DROP 红线经 20260902 重构方案 §7.1 豁免） ═══════════

BEGIN;

SET LOCAL lock_timeout = '5s';

DROP TABLE IF EXISTS cmx_flow_webhook_subscription;
DROP TABLE IF EXISTS cmx_flow_webhook_delivery;
ALTER TABLE cmx_flow_instance    DROP COLUMN IF EXISTS subscriber_id;
ALTER TABLE cmx_flow_hi_instance DROP COLUMN IF EXISTS subscriber_id;

COMMIT;

-- ═══════════ 二、流程分组 + 定义分组列（重构方案 §3.1/§3.2） ═══════════

-- 流程分组表：一级扁平（非树）；enabled 仅影响定义页展示位，永不参与运行时匹配（硬承诺）。
CREATE TABLE IF NOT EXISTS cmx_flow_def_group (
    id         BIGINT       NOT NULL,
    name       VARCHAR(64)  NOT NULL,
    sort_no    INT          NOT NULL DEFAULT 0,
    enabled    BOOLEAN      NOT NULL DEFAULT TRUE,
    remark     VARCHAR(512),
    created_at TIMESTAMPTZ  NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ  NOT NULL DEFAULT now(),
    PRIMARY KEY (id)
);
COMMENT ON TABLE  cmx_flow_def_group            IS '流程定义分组（重构方案 §3.1）：一级扁平；定义列表页左侧面板的归属维度与订阅规则 groupIds 的匹配维度';
COMMENT ON COLUMN cmx_flow_def_group.id         IS '主键（应用层 Pk52 雪花，52 位 JS Number 安全）';
COMMENT ON COLUMN cmx_flow_def_group.name       IS '分组名（全局唯一）';
COMMENT ON COLUMN cmx_flow_def_group.sort_no    IS '展示序（定义页上移下移改此列；纯展示，不参与匹配）';
COMMENT ON COLUMN cmx_flow_def_group.enabled    IS '启用位（仅定义页展示位：停用组折叠置灰；永不参与运行时路由）';
COMMENT ON COLUMN cmx_flow_def_group.remark     IS '备注';
COMMENT ON COLUMN cmx_flow_def_group.updated_at IS '最近更新时间（DB 时钟；进 EventRouteCache 指纹对账）';
CREATE UNIQUE INDEX IF NOT EXISTS uq_cmx_flow_def_group_name ON cmx_flow_def_group (name);

-- 定义表补分组列（NULL = 未分组；分组是唯一新增归属维度，DAM 三段列冻结不动）。
BEGIN;

SET LOCAL lock_timeout = '5s';

ALTER TABLE cmx_flow_definition ADD COLUMN IF NOT EXISTS group_id BIGINT;
COMMENT ON COLUMN cmx_flow_definition.group_id IS '所属流程分组 id → cmx_flow_def_group.id（NULL = 未分组；订阅规则 groupIds 匹配维度）';
CREATE INDEX IF NOT EXISTS idx_cmx_flow_definition_group ON cmx_flow_definition (group_id);

COMMIT;

-- ═══════════ 三、事件订阅者表（重构方案 §3.3：rules JSONB 内嵌单行） ═══════════

CREATE TABLE IF NOT EXISTS cmx_flow_event_subscriber (
    id             BIGINT       NOT NULL,
    name           VARCHAR(128) NOT NULL,
    description    VARCHAR(512),
    channel        VARCHAR(16)  NOT NULL DEFAULT 'webhook',
    channel_config JSONB        NOT NULL DEFAULT '{}',
    rules          JSONB        NOT NULL DEFAULT '[]',
    retry_max      INT          NOT NULL DEFAULT 10,
    active         BOOLEAN      NOT NULL DEFAULT TRUE,
    tenant_id      VARCHAR(64)  NOT NULL DEFAULT 'default',
    created_by     VARCHAR(64),
    created_at     TIMESTAMPTZ  NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ  NOT NULL DEFAULT now(),
    PRIMARY KEY (id)
);
COMMENT ON TABLE  cmx_flow_event_subscriber                IS '事件订阅者（重构方案 §3.3）：注册回调方 + 多条订阅规则内嵌 rules JSONB（逻辑两级、物理单行——save 单行 upsert 天然原子，数组序即命中序）';
COMMENT ON COLUMN cmx_flow_event_subscriber.name           IS '订阅者名（uk：同租户唯一）';
COMMENT ON COLUMN cmx_flow_event_subscriber.description    IS '描述';
COMMENT ON COLUMN cmx_flow_event_subscriber.channel        IS '通道类型：webhook（kafka/rabbitmq feature 预留，save 时校验注册表已注册）';
COMMENT ON COLUMN cmx_flow_event_subscriber.channel_config IS '通道配置（开放对象）：webhook {service_key, callback_path, secret——secret 明文，API 掩码回显}；MQ 只存路由目标，连接凭据走 toml';
COMMENT ON COLUMN cmx_flow_event_subscriber.rules          IS '订阅规则数组（元素 {name≤64 同订阅者内唯一, enabled, eventTypes[], groupIds[], keyPatterns[]}）：规则内三维 AND、跨规则 OR、数组序=命中序；全空规则=匹配全部（网关形态）';
COMMENT ON COLUMN cmx_flow_event_subscriber.retry_max      IS '最大尝试次数（含首发，对齐 mdm 口径）；默认 10 = 重试 9 次';
COMMENT ON COLUMN cmx_flow_event_subscriber.active         IS '启停：停用即不再生成投递行（存量行保留可查可清）';
COMMENT ON COLUMN cmx_flow_event_subscriber.tenant_id      IS '租户（db-per-tenant 下冗余登记）';
COMMENT ON COLUMN cmx_flow_event_subscriber.created_by     IS '创建人';
COMMENT ON COLUMN cmx_flow_event_subscriber.updated_at     IS '最近更新时间（DB 时钟；缓存指纹对账列——set-active 等 UPDATE 必带 now()）';
CREATE UNIQUE INDEX IF NOT EXISTS uq_cmx_flow_event_sub_name ON cmx_flow_event_subscriber (tenant_id, name);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_event_sub_upd ON cmx_flow_event_subscriber (updated_at);

-- ═══════════ 四、事件投递表（重构方案 §3.4：租约/保序/死信机制继承，删 route_source，加 matched_rule） ═══════════

CREATE TABLE IF NOT EXISTS cmx_flow_event_delivery (
    id                    BIGINT        NOT NULL,
    seq                   BIGSERIAL     NOT NULL,
    subscriber_id         BIGINT        NOT NULL,
    subscriber_name       VARCHAR(128)  NOT NULL,
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
    matched_rule          VARCHAR(64),
    created_at            TIMESTAMPTZ   NOT NULL DEFAULT now(),
    delivered_at          TIMESTAMPTZ,
    PRIMARY KEY (id)
);
COMMENT ON TABLE  cmx_flow_event_delivery                     IS '事件持久化投递队列（事件×订阅者×命中规则一行；PENDING/IN_FLIGHT/DONE/DEAD/SKIPPED 状态机 + 死信；租约抢占 + 同订阅者保序）';
COMMENT ON COLUMN cmx_flow_event_delivery.seq                 IS '保序键（BIGSERIAL，DB 按提交序赋值；同订阅者按 seq 严格保序，雪花不做保序）';
COMMENT ON COLUMN cmx_flow_event_delivery.subscriber_id       IS '归属订阅者 id → cmx_flow_event_subscriber.id';
COMMENT ON COLUMN cmx_flow_event_delivery.subscriber_name     IS '订阅者名快照（写入时定版；订阅者删除/改名后流水仍可辨识）';
COMMENT ON COLUMN cmx_flow_event_delivery.channel             IS '通道快照（写入时定版）';
COMMENT ON COLUMN cmx_flow_event_delivery.event_id            IS '事件唯一键（emit 时 uuid；rebuild 确定性 rb- 前缀；uk(订阅者,事件) 幂等仅约束 rebuild/test 重复点击）';
COMMENT ON COLUMN cmx_flow_event_delivery.delivery_id         IS 'wire 幂等参考键 {instanceId}-{taskId?}-{occurredAt}（x-cmx-flow-delivery 头）';
COMMENT ON COLUMN cmx_flow_event_delivery.source              IS '来源：emit 业务事件 / test 测试 / rebuild 补发（test 直达终态不参与保序）';
COMMENT ON COLUMN cmx_flow_event_delivery.event_type          IS '事件类型（instance.started 等 6 种 / webhook.test 伪事件）';
COMMENT ON COLUMN cmx_flow_event_delivery.definition_key      IS '流程定义 key（冗余查询列）';
COMMENT ON COLUMN cmx_flow_event_delivery.business_key        IS '业务键（冗余查询列）';
COMMENT ON COLUMN cmx_flow_event_delivery.instance_id         IS '实例 id（冗余查询列）';
COMMENT ON COLUMN cmx_flow_event_delivery.payload             IS '完整事件体 JSONB（投递 body 即此；additive 字段 systemId/groupId/groupName 可空）';
COMMENT ON COLUMN cmx_flow_event_delivery.state               IS '状态：PENDING 待投 / IN_FLIGHT 投递中 / DONE 成功 / DEAD 死信 / SKIPPED 人工处置；终态均不阻塞同订阅者保序';
COMMENT ON COLUMN cmx_flow_event_delivery.attempts            IS '已尝试次数（claim 时 +1；retry_max 含首发）';
COMMENT ON COLUMN cmx_flow_event_delivery.next_attempt_at     IS '退避到期时间（1s 起指数封顶 5min）';
COMMENT ON COLUMN cmx_flow_event_delivery.locked_by           IS '租约持有者（worker id；多副本投递互斥）';
COMMENT ON COLUMN cmx_flow_event_delivery.lock_expires_at     IS '租约到期（120s；逐行续租，过期可被重抢自愈）';
COMMENT ON COLUMN cmx_flow_event_delivery.last_error          IS '最近失败原因（死信诊断）';
COMMENT ON COLUMN cmx_flow_event_delivery.last_http_status    IS '最近 HTTP 状态码（死信诊断）';
COMMENT ON COLUMN cmx_flow_event_delivery.last_response_snippet IS '响应摘要（截断 512，死信诊断）';
COMMENT ON COLUMN cmx_flow_event_delivery.matched_rule        IS '命中规则名快照（emit 时定版，列宽与规则名校验上限同为 64；test/rebuild 行 NULL）';
COMMENT ON COLUMN cmx_flow_event_delivery.created_at          IS '创建时间（stats 时间窗扫描索引列）';
COMMENT ON COLUMN cmx_flow_event_delivery.delivered_at        IS '投递成功时间';
CREATE UNIQUE INDEX IF NOT EXISTS uq_cmx_flow_event_dlv_seq       ON cmx_flow_event_delivery (seq);
CREATE UNIQUE INDEX IF NOT EXISTS uq_cmx_flow_event_dlv_sub_event ON cmx_flow_event_delivery (subscriber_id, event_id);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_event_dlv_due    ON cmx_flow_event_delivery (state, next_attempt_at);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_event_dlv_sub    ON cmx_flow_event_delivery (subscriber_id, seq);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_event_dlv_did    ON cmx_flow_event_delivery (delivery_id);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_event_dlv_created ON cmx_flow_event_delivery (created_at);

-- ═══════════ 五、实例乐观锁与系统归属列（原 20260902_002，技术债 007 + 005，逐字节保留） ═══════════

-- 007：cmx_flow_instance 加 version 乐观锁（save 以 WHERE id AND version CAS 提交并 +1；
--       配合代码侧 cc/转签台账旁路剥离——两表不再随快照 DELETE 重插，此迁移不涉及）。
-- 005：instance/hi_instance 加 system_id（结构化 API Key 声明的调用方系统归属）。
BEGIN;

SET LOCAL lock_timeout = '5s';

ALTER TABLE cmx_flow_instance    ADD COLUMN IF NOT EXISTS version   BIGINT NOT NULL DEFAULT 0;
ALTER TABLE cmx_flow_instance    ADD COLUMN IF NOT EXISTS system_id VARCHAR(64);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_instance_system ON cmx_flow_instance (system_id);

ALTER TABLE cmx_flow_hi_instance ADD COLUMN IF NOT EXISTS version   BIGINT NOT NULL DEFAULT 0;
ALTER TABLE cmx_flow_hi_instance ADD COLUMN IF NOT EXISTS system_id VARCHAR(64);

COMMENT ON COLUMN cmx_flow_instance.version   IS '乐观锁版本（技术债 007）：save 以 WHERE id AND version CAS 提交并 +1，0 行即并发冲突 409';
COMMENT ON COLUMN cmx_flow_instance.system_id IS '发起方业务系统标识（技术债 005：来自结构化 API Key 声明；NULL = legacy 调用未声明系统）；子实例继承';
COMMENT ON COLUMN cmx_flow_hi_instance.version   IS '归档时的乐观锁版本（技术债 007 审计留档）';
COMMENT ON COLUMN cmx_flow_hi_instance.system_id IS '发起方业务系统标识（技术债 005 归档登记）';

COMMIT;

-- ═══════════ 六、定时器租约列 + 故障清单表（原 20260902_003，技术债 008 + 011，逐字节保留） ═══════════

-- 008：cmx_flow_job 加租约列（SKIP LOCKED 抢占——多副本下同一作业只被一个副本 fire）。
-- 011：cmx_flow_incident 独立故障清单表（跨实例台账，/incidents 端点 + 自动重试数据源）。
BEGIN;

SET LOCAL lock_timeout = '5s';

ALTER TABLE cmx_flow_job ADD COLUMN IF NOT EXISTS claimed_by VARCHAR(128);
ALTER TABLE cmx_flow_job ADD COLUMN IF NOT EXISTS lease_expires_at TIMESTAMPTZ;
CREATE INDEX IF NOT EXISTS idx_cmx_flow_job_acquire ON cmx_flow_job (due_at, claimed_by, lease_expires_at);

COMMENT ON COLUMN cmx_flow_job.claimed_by       IS '定时器抢占持有者（技术债 008；worker id = timer-<pid>）';
COMMENT ON COLUMN cmx_flow_job.lease_expires_at IS '租约到期时刻（到期后作业可被其它副本重抢）';

CREATE TABLE IF NOT EXISTS cmx_flow_incident (
    id              VARCHAR(64)  PRIMARY KEY,
    instance_id     VARCHAR(64)  NOT NULL,
    token_id        VARCHAR(64),
    node_bpmn_id    VARCHAR(128) NOT NULL,
    definition_key  VARCHAR(128) NOT NULL,
    business_key    VARCHAR(128),
    reason          TEXT         NOT NULL DEFAULT '',
    retries         INTEGER      NOT NULL DEFAULT 0,
    state           VARCHAR(16)  NOT NULL DEFAULT 'OPEN',
    created_at      TIMESTAMPTZ  NOT NULL,
    updated_at      TIMESTAMPTZ  NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS uq_cmx_flow_incident_inst_node ON cmx_flow_incident (instance_id, node_bpmn_id);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_incident_state ON cmx_flow_incident (state, updated_at);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_incident_def ON cmx_flow_incident (definition_key);

COMMENT ON TABLE  cmx_flow_incident           IS '流程故障清单（技术债 011：跨实例 incident 台账；实例变量 __incident 仍是实例内派生视图）';
COMMENT ON COLUMN cmx_flow_incident.state     IS 'OPEN / RESOLVED（retry_incident 成功后批量关闭）';
COMMENT ON COLUMN cmx_flow_incident.retries   IS '累计发生/重试次数（同 instance+node 幂等累加）';

COMMIT;
