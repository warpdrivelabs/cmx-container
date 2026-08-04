-- cmx-flow 表单集成 F1/F3：单据↔实例关联 + 任务意见留痕
-- 幂等：CREATE TABLE / INDEX IF NOT EXISTS。
-- 约束遵循：cmx_ 前缀；无 FOREIGN KEY（软关联 + 索引替代）；行 id 用 uuid 字符串。
-- 依赖：cmx_flow_instance（20260720_001_cmx_flow_engine）—— 反查关联 JOIN 用。
-- 说明：
--   cmx_flow_biz_link     —— 发起时把业务单据(表+主键)关联到实例；双向查询（正向实例→单据、反向单据→实例）。
--                            一单可挂多流程（role 区分），故非唯一约束到 instance，而是 (biz_table,biz_id,instance_id) 唯一。
--   cmx_flow_task_comment —— 办结时按环节留痕审批意见；变量会被同名 merge 覆盖，不适合当历史，故独立成表。

CREATE TABLE IF NOT EXISTS cmx_flow_biz_link (
    id           VARCHAR(64)  PRIMARY KEY,
    instance_id  VARCHAR(64)  NOT NULL,
    biz_table    VARCHAR(128) NOT NULL,
    biz_id       VARCHAR(128) NOT NULL,
    biz_key      VARCHAR(128),
    role         VARCHAR(32)  NOT NULL DEFAULT 'primary',
    created_at   TIMESTAMPTZ  NOT NULL
);
COMMENT ON TABLE  cmx_flow_biz_link              IS '单据↔流程实例关联（F1；发起时回写，双向可查）';
COMMENT ON COLUMN cmx_flow_biz_link.biz_table    IS '业务表名（如 cf_pay_request）';
COMMENT ON COLUMN cmx_flow_biz_link.biz_id       IS '业务单据主键（字符串兼容 bigint/code/uuid）';
COMMENT ON COLUMN cmx_flow_biz_link.role         IS '关联角色：primary 主单 / 其它扩展（一单多流程时区分）';
CREATE INDEX IF NOT EXISTS idx_cmx_flow_biz_link_instance ON cmx_flow_biz_link (instance_id);
CREATE UNIQUE INDEX IF NOT EXISTS uq_cmx_flow_biz_link_biz ON cmx_flow_biz_link (biz_table, biz_id, instance_id);

CREATE TABLE IF NOT EXISTS cmx_flow_task_comment (
    id           VARCHAR(64)  PRIMARY KEY,
    instance_id  VARCHAR(64)  NOT NULL,
    task_id      VARCHAR(64)  NOT NULL,
    node_bpmn_id VARCHAR(128),
    user_id      VARCHAR(64),
    decision     VARCHAR(32),
    comment      TEXT,
    created_at   TIMESTAMPTZ  NOT NULL
);
COMMENT ON TABLE  cmx_flow_task_comment          IS '审批意见留痕（F3；办结时按环节记，供表单审批区展示历史）';
COMMENT ON COLUMN cmx_flow_task_comment.decision IS '决策：approve / reject 等';
CREATE INDEX IF NOT EXISTS idx_cmx_flow_task_comment_instance ON cmx_flow_task_comment (instance_id);
