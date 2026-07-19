-- cmx-flow 可视化设计器 阶段0：流程定义持久化层
-- 幂等：CREATE TABLE / INDEX IF NOT EXISTS。
-- 约束遵循：cmx_ 前缀；无 FOREIGN KEY（def_key 关联 + 索引替代）。
-- 依赖：无（独立于运行态表；与 cmx_flow_instance 等无耦合）。
-- 涉及变更：新增两表，支撑「设计器画完存草稿 → 发布 → 引擎从库装载」。
-- 说明：
--   - cmx_flow_definition：定义主记录（当前指针）。draft_xml 存当前草稿的 BPMN；
--     active_version 指向已发布版本；state = DRAFT / PUBLISHED。一个 key 一行。
--   - cmx_flow_definition_version：版本历史（不可变），每次 publish 追加一行 bpmn_xml 快照。
--     (def_key, version) 唯一。历史实例仍可按旧版定义解释，故版本不删。
--   - 存 XML 而非编译后 IR：XML 是标准交换格式，可重新加载编辑/审阅/版本 diff；引擎装载时 compile。

CREATE TABLE IF NOT EXISTS cmx_flow_definition (
    key            VARCHAR(128) PRIMARY KEY,
    name           VARCHAR(255) NOT NULL,
    module         VARCHAR(64),
    category       VARCHAR(64),
    state          VARCHAR(16)  NOT NULL DEFAULT 'DRAFT',
    active_version INTEGER,
    draft_xml      TEXT,
    updated_at     TIMESTAMPTZ  NOT NULL,
    updated_by     VARCHAR(64)
);
COMMENT ON TABLE  cmx_flow_definition                IS '流程定义主记录（当前指针：草稿 XML + 已发布版本指向）';
COMMENT ON COLUMN cmx_flow_definition.key            IS '流程定义 key（= BPMN process id）';
COMMENT ON COLUMN cmx_flow_definition.state          IS '状态：DRAFT / PUBLISHED';
COMMENT ON COLUMN cmx_flow_definition.active_version IS '当前已发布版本号（未发布为 NULL）';
COMMENT ON COLUMN cmx_flow_definition.draft_xml      IS '当前草稿的 BPMN XML（设计器产物）';
CREATE INDEX IF NOT EXISTS idx_cmx_flow_definition_module ON cmx_flow_definition (module);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_definition_state ON cmx_flow_definition (state);

CREATE TABLE IF NOT EXISTS cmx_flow_definition_version (
    id            VARCHAR(64)  PRIMARY KEY,
    def_key       VARCHAR(128) NOT NULL,
    version       INTEGER      NOT NULL,
    bpmn_xml      TEXT         NOT NULL,
    published_at  TIMESTAMPTZ  NOT NULL,
    published_by  VARCHAR(64)
);
COMMENT ON TABLE  cmx_flow_definition_version         IS '流程定义版本历史（不可变，每次发布追加 BPMN 快照）';
COMMENT ON COLUMN cmx_flow_definition_version.version IS '版本号（同 def_key 下从 1 递增）';
CREATE UNIQUE INDEX IF NOT EXISTS uq_cmx_flow_def_version ON cmx_flow_definition_version (def_key, version);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_def_version_key ON cmx_flow_definition_version (def_key);
