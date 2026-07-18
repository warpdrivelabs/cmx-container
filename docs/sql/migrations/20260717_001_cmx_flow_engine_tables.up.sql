-- cmx-flow 流程引擎运行态与历史态表（M1 + M2）
-- 幂等：全部 CREATE TABLE IF NOT EXISTS / CREATE INDEX IF NOT EXISTS。
-- 约束遵循：cmx_ 前缀；无 FOREIGN KEY（关联字段 + 索引替代）；标准 TIMESTAMPTZ。
-- 涉及表：
--   运行态(RU)：cmx_flow_instance / cmx_flow_token / cmx_flow_task
--   历史态(HI)：cmx_flow_hi_instance / cmx_flow_hi_task
-- 说明：实例进入终态（COMPLETED/TERMINATED）时，由 cmx-flow-store-pg 在同事务内
--       归档到 HI 表（幂等 upsert）。RU/HI 分离对齐 Flowable 的运行态/历史态设计。

-- =============================================
-- 1. 流程实例表（聚合根）
-- =============================================
CREATE TABLE IF NOT EXISTS cmx_flow_instance
(
    id             VARCHAR(64)  NOT NULL,
    definition_key VARCHAR(128) NOT NULL,
    business_key   VARCHAR(128),
    state          VARCHAR(16)  NOT NULL,
    variables      JSONB        NOT NULL DEFAULT '{}'::jsonb,
    created_at     TIMESTAMPTZ  NOT NULL,
    updated_at     TIMESTAMPTZ  NOT NULL,
    ended_at       TIMESTAMPTZ,
    PRIMARY KEY (id)
);
COMMENT ON TABLE  cmx_flow_instance                IS '流程实例（运行态聚合根）';
COMMENT ON COLUMN cmx_flow_instance.definition_key IS '流程定义 key（BPMN process id）';
COMMENT ON COLUMN cmx_flow_instance.business_key   IS '业务键，对接业务单据（可空）';
COMMENT ON COLUMN cmx_flow_instance.state          IS '实例状态：ACTIVE / COMPLETED / TERMINATED';
COMMENT ON COLUMN cmx_flow_instance.variables      IS '实例级流程变量（JSONB 动态 KV）';
CREATE INDEX IF NOT EXISTS idx_cmx_flow_instance_defkey ON cmx_flow_instance (definition_key);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_instance_bizkey ON cmx_flow_instance (business_key);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_instance_state  ON cmx_flow_instance (state);

-- =============================================
-- 2. 令牌表（流经流程图的执行指针）
-- =============================================
CREATE TABLE IF NOT EXISTS cmx_flow_token
(
    id           VARCHAR(64)  NOT NULL,
    instance_id  VARCHAR(64)  NOT NULL,
    node_bpmn_id VARCHAR(128) NOT NULL,
    state        VARCHAR(16)  NOT NULL,
    parent_id    VARCHAR(64),
    created_at   TIMESTAMPTZ  NOT NULL,
    updated_at   TIMESTAMPTZ  NOT NULL,
    PRIMARY KEY (id)
);
COMMENT ON TABLE  cmx_flow_token              IS '流程令牌（执行指针；一实例多令牌，无外键关联 instance）';
COMMENT ON COLUMN cmx_flow_token.instance_id  IS '所属实例 id（逻辑关联 cmx_flow_instance.id）';
COMMENT ON COLUMN cmx_flow_token.node_bpmn_id IS '当前所在节点的 BPMN id（稳定锚点）';
COMMENT ON COLUMN cmx_flow_token.state        IS '令牌状态：ACTIVE / WAITING / JOINING / ENDED';
COMMENT ON COLUMN cmx_flow_token.parent_id    IS '父令牌 id（并行网关 fork 分裂血缘；可空）';
CREATE INDEX IF NOT EXISTS idx_cmx_flow_token_instance ON cmx_flow_token (instance_id);

-- =============================================
-- 3. 用户任务表（等待态外化）
-- =============================================
CREATE TABLE IF NOT EXISTS cmx_flow_task
(
    id               VARCHAR(64)  NOT NULL,
    instance_id      VARCHAR(64)  NOT NULL,
    token_id         VARCHAR(64)  NOT NULL,
    node_bpmn_id     VARCHAR(128) NOT NULL,
    name             VARCHAR(255),
    assignee         VARCHAR(128),
    candidate_groups VARCHAR(512),
    completed        BOOLEAN      NOT NULL DEFAULT FALSE,
    created_at       TIMESTAMPTZ  NOT NULL,
    completed_at     TIMESTAMPTZ,
    PRIMARY KEY (id)
);
COMMENT ON TABLE  cmx_flow_task                  IS '用户任务（userTask 等待态的外化产物）';
COMMENT ON COLUMN cmx_flow_task.instance_id      IS '所属实例 id（逻辑关联 cmx_flow_instance.id）';
COMMENT ON COLUMN cmx_flow_task.token_id         IS '产生该任务的令牌 id（逻辑关联 cmx_flow_token.id）';
COMMENT ON COLUMN cmx_flow_task.node_bpmn_id     IS '对应 userTask 节点的 BPMN id';
COMMENT ON COLUMN cmx_flow_task.candidate_groups IS '候选组（逗号分隔，M2 未解析）';
CREATE INDEX IF NOT EXISTS idx_cmx_flow_task_instance ON cmx_flow_task (instance_id);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_task_assignee ON cmx_flow_task (assignee);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_task_open     ON cmx_flow_task (assignee, completed);

-- =============================================
-- 4. 历史实例表（HI：终态归档，与热运行态解耦）
-- =============================================
CREATE TABLE IF NOT EXISTS cmx_flow_hi_instance
(
    id             VARCHAR(64)  NOT NULL,
    definition_key VARCHAR(128) NOT NULL,
    business_key   VARCHAR(128),
    state          VARCHAR(16)  NOT NULL,
    variables      JSONB        NOT NULL DEFAULT '{}'::jsonb,
    created_at     TIMESTAMPTZ  NOT NULL,
    ended_at       TIMESTAMPTZ,
    duration_ms    BIGINT,
    archived_at    TIMESTAMPTZ  NOT NULL,
    PRIMARY KEY (id)
);
COMMENT ON TABLE  cmx_flow_hi_instance             IS '历史流程实例（终态归档，供审计/查询）';
COMMENT ON COLUMN cmx_flow_hi_instance.duration_ms IS '实例存续时长（ended_at - created_at，毫秒）';
COMMENT ON COLUMN cmx_flow_hi_instance.archived_at IS '归档写入时刻';
CREATE INDEX IF NOT EXISTS idx_cmx_flow_hi_instance_defkey ON cmx_flow_hi_instance (definition_key);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_hi_instance_bizkey ON cmx_flow_hi_instance (business_key);

-- =============================================
-- 5. 历史任务表（HI：办结任务归档，含耗时）
-- =============================================
CREATE TABLE IF NOT EXISTS cmx_flow_hi_task
(
    id           VARCHAR(64)  NOT NULL,
    instance_id  VARCHAR(64)  NOT NULL,
    node_bpmn_id VARCHAR(128) NOT NULL,
    name         VARCHAR(255),
    assignee     VARCHAR(128),
    created_at   TIMESTAMPTZ  NOT NULL,
    completed_at TIMESTAMPTZ,
    duration_ms  BIGINT,
    archived_at  TIMESTAMPTZ  NOT NULL,
    PRIMARY KEY (id)
);
COMMENT ON TABLE  cmx_flow_hi_task             IS '历史用户任务（办结归档，供工时分析/审计）';
COMMENT ON COLUMN cmx_flow_hi_task.instance_id IS '所属实例 id（逻辑关联 cmx_flow_hi_instance.id）';
COMMENT ON COLUMN cmx_flow_hi_task.duration_ms IS '任务办理时长（completed_at - created_at，毫秒）';
COMMENT ON COLUMN cmx_flow_hi_task.archived_at IS '归档写入时刻';
CREATE INDEX IF NOT EXISTS idx_cmx_flow_hi_task_instance ON cmx_flow_hi_task (instance_id);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_hi_task_assignee ON cmx_flow_hi_task (assignee);
