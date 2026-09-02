-- 20260902_003: 定时器租约列 + 故障清单表（技术债 008 + 011，治理方案批次 5/6）
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
