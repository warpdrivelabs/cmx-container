-- cmx-flow 流程引擎 M5.2：子流程组织路由绑定表
-- 幂等：CREATE TABLE / INDEX IF NOT EXISTS。
-- 约束遵循：cmx_ 前缀；无 FOREIGN KEY（关联字段 + 索引替代）。
-- 依赖：20260718_001（cmx_org 组织树，含物化 path）、20260718_004（M5.1 子流程）。
-- 说明：
--   - 主流程 callActivity 用「逻辑 key」（cmx:calledKey，如 fin_review），不写死具体子流程；
--   - 各组织把「逻辑 key + 本组织 → 具体子流程定义 key」绑定在本表；
--   - 运行期 PgSubflowRouter 三层解析：① 精确(本组织绑定) ② 继承(沿 cmx_org.path 向上找最近
--     祖先绑定) ③ 兜底(org_id IS NULL 的默认绑定)；
--   - org_id 为 NULL = 默认兜底绑定；enabled=FALSE 的绑定不参与解析。
--   - 与 cmx_org 应在同一库（PgSubflowRouter 的 db_id 指向该库）。

CREATE TABLE IF NOT EXISTS cmx_flow_subflow_binding
(
    id                    VARCHAR(64)  NOT NULL,
    called_key            VARCHAR(128) NOT NULL,
    org_id                VARCHAR(64),
    target_definition_key VARCHAR(128) NOT NULL,
    enabled               BOOLEAN      NOT NULL DEFAULT TRUE,
    remark                VARCHAR(500),
    created_at            TIMESTAMPTZ  NOT NULL DEFAULT now(),
    updated_at            TIMESTAMPTZ  NOT NULL DEFAULT now(),
    PRIMARY KEY (id)
);
COMMENT ON TABLE  cmx_flow_subflow_binding                       IS '子流程组织绑定（逻辑 key + 组织 → 具体子流程定义；M5.2 路由数据源）';
COMMENT ON COLUMN cmx_flow_subflow_binding.called_key            IS '主流程 callActivity 上的逻辑子流程 key（如 fin_review）';
COMMENT ON COLUMN cmx_flow_subflow_binding.org_id               IS '适用组织（NULL = 默认兜底绑定）';
COMMENT ON COLUMN cmx_flow_subflow_binding.target_definition_key IS '解析到的具体子流程定义 key';
COMMENT ON COLUMN cmx_flow_subflow_binding.enabled              IS '是否启用（FALSE 不参与解析）';
CREATE INDEX IF NOT EXISTS idx_cmx_flow_subflow_binding_key ON cmx_flow_subflow_binding (called_key);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_subflow_binding_org ON cmx_flow_subflow_binding (org_id);
