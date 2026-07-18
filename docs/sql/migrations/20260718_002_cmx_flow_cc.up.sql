-- cmx-flow 流程引擎 M4.2：抄送记录表（知会 + 已读）
-- 幂等：CREATE TABLE / INDEX IF NOT EXISTS。
-- 约束遵循：cmx_ 前缀；无 FOREIGN KEY（关联字段 + 索引替代）；标准 TIMESTAMPTZ。
-- 依赖：20260717_001（M1+M2 基础表）。
-- 说明：抄送是流程的只读旁路——不阻塞流程、不产生待办、不影响令牌推进。每个被抄送人一条，
--       带已读追踪（read_at）。触发：节点配置 cmx:cc（任务办结时）或办理人手动抄送（notify_cc）。
--       随实例聚合快照全删重插；实例终态时随聚合归档（记录保留在 cmx_flow_cc，供审计）。
--       idx_..._to_user (to_user_id, read_at) 支撑「抄送我的」+ 未读过滤。

CREATE TABLE IF NOT EXISTS cmx_flow_cc
(
    id            VARCHAR(64)  NOT NULL,
    instance_id   VARCHAR(64)  NOT NULL,
    node_bpmn_id  VARCHAR(128),
    to_user_id    VARCHAR(64)  NOT NULL,
    from_user_id  VARCHAR(64),
    reason        VARCHAR(500),
    read_at       TIMESTAMPTZ,
    created_at    TIMESTAMPTZ  NOT NULL,
    PRIMARY KEY (id)
);
COMMENT ON TABLE  cmx_flow_cc              IS '抄送记录表（只读知会 + 已读追踪；不阻塞流程）';
COMMENT ON COLUMN cmx_flow_cc.node_bpmn_id IS '抄送发生的节点（可空；手动抄送为 NULL）';
COMMENT ON COLUMN cmx_flow_cc.to_user_id   IS '被抄送人 user id';
COMMENT ON COLUMN cmx_flow_cc.from_user_id IS '抄送发起人 user id（办理人；节点自动抄送可空）';
COMMENT ON COLUMN cmx_flow_cc.read_at      IS '已读时刻（NULL = 未读）';
CREATE INDEX IF NOT EXISTS idx_cmx_flow_cc_instance ON cmx_flow_cc (instance_id);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_cc_to_user  ON cmx_flow_cc (to_user_id, read_at);
