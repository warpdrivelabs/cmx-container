-- cmx-flow 子流程组织绑定表（生产库补建）
-- 幂等：CREATE TABLE / INDEX IF NOT EXISTS。
-- 背景：M5.2 的 cmx_flow_subflow_binding 原来只由 demo 的 CREATE TABLE 种入 cmx 库；
--   生产 web-server 从未建过（引擎 ensure_schema 只覆盖 fico-db 运行态表）。
--   设计器要能「按组织配置子流程」，此表须在 IAM/主库（primary = cmx）存在。
-- 库：primary（cmx）——与 cmx_org 同库，PgSubflowRouter / PgSubflowBindingStore 都指 IAM_DB_ID。
-- 说明：called_key = callActivity 的 cmx:calledKey（逻辑子流程名）；org_id 为空 = 默认兜底绑定；
--   运行期 PgSubflowRouter 三层解析（精确 org → 沿 cmx_org.path 继承 → 兜底）。

CREATE TABLE IF NOT EXISTS cmx_flow_subflow_binding (
    id                    VARCHAR(64)  PRIMARY KEY,
    called_key            VARCHAR(128) NOT NULL,
    org_id                VARCHAR(64),
    target_definition_key VARCHAR(128) NOT NULL,
    enabled               BOOLEAN      NOT NULL DEFAULT TRUE,
    remark                VARCHAR(500),
    created_at            TIMESTAMPTZ  NOT NULL DEFAULT now(),
    updated_at            TIMESTAMPTZ  NOT NULL DEFAULT now()
);
COMMENT ON TABLE  cmx_flow_subflow_binding                       IS '子流程组织绑定（逻辑 key + 组织 → 具体子流程定义）；定义态配置，运行期 SubflowRouter 解析';
COMMENT ON COLUMN cmx_flow_subflow_binding.called_key            IS 'callActivity 的 cmx:calledKey（逻辑子流程名）';
COMMENT ON COLUMN cmx_flow_subflow_binding.org_id                IS '组织 id（NULL = 默认兜底绑定）';
COMMENT ON COLUMN cmx_flow_subflow_binding.target_definition_key IS '目标子流程定义 key';
CREATE INDEX IF NOT EXISTS idx_cmx_flow_subflow_binding_key ON cmx_flow_subflow_binding (called_key);
