-- =====================================================
-- cmx-code 编码引擎（两张表合并迁移）
-- 1. cmx_code_rule  —— 编码规则库（纯算法：段序列，不带 target，可被多处复用）
-- 2. cmx_code_gap   —— 编码断号表（连号域空缺号回收，只存空缺 ≠ 已分配）
-- 规则按域/应用/模块（DAM）隔离，既有规则无 DAM 默认空串，兼容存量
-- =====================================================

-- ─────────────────────────────────────────────────────
-- 1. 编码规则库
-- ─────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS cmx_code_rule (
    id              BIGINT                  NOT NULL,
    rule_code       VARCHAR(64)             NOT NULL,
    rule_name       VARCHAR(128)            NOT NULL,
    mode            VARCHAR(16)             NOT NULL DEFAULT 'auto',
    org_scope       VARCHAR(64),
    condition       TEXT,
    segments        JSONB                   NOT NULL DEFAULT '[]',
    joiner          VARCHAR(4)              NOT NULL DEFAULT '',
    pattern         TEXT,
    enable_gap      BOOLEAN                 NOT NULL DEFAULT FALSE,
    use_sequence    BOOLEAN                 NOT NULL DEFAULT FALSE,
    valid_from      DATE,
    valid_to        DATE,
    priority        INT4                    NOT NULL DEFAULT 100,
    is_active       BOOLEAN                 NOT NULL DEFAULT TRUE,
    -- DAM 维度（域/应用/模块隔离，空串=兼容存量/全局可见）
    domain_code     VARCHAR(32)             NOT NULL DEFAULT '',
    application_code VARCHAR(32)            NOT NULL DEFAULT '',
    module_code     VARCHAR(32)             NOT NULL DEFAULT '',
    create_time     TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    update_time     TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    archived        INT4                    NOT NULL DEFAULT 0,
    create_by       VARCHAR(100),
    update_by       VARCHAR(100),
    PRIMARY KEY (id)
);

CREATE UNIQUE INDEX IF NOT EXISTS uk_cmx_code_rule_rule_code ON cmx_code_rule (rule_code) WHERE archived = 0;
CREATE INDEX IF NOT EXISTS ix_cmx_code_rule_active ON cmx_code_rule (is_active, priority);
CREATE INDEX IF NOT EXISTS ix_cmx_code_rule_archived ON cmx_code_rule (archived);
-- DAM + archived 复合索引：按模块过滤规则列表的主查询路径
CREATE INDEX IF NOT EXISTS ix_cmx_code_rule_dam ON cmx_code_rule (domain_code, application_code, module_code, archived);

COMMENT ON TABLE cmx_code_rule IS '编码规则库（纯算法，不带 target，可被多处复用）';
COMMENT ON COLUMN cmx_code_rule.id IS '主键ID（pk52）';
COMMENT ON COLUMN cmx_code_rule.rule_code IS '规则码（人类可读，全局唯一，如 supplier_hq）';
COMMENT ON COLUMN cmx_code_rule.rule_name IS '规则名称（展示用）';
COMMENT ON COLUMN cmx_code_rule.mode IS '模式：auto（引擎生成）| manual（用户手敲，引擎只校验）';
COMMENT ON COLUMN cmx_code_rule.org_scope IS '受控组织（可选，逗号分隔多组织，组织命中才生效）';
COMMENT ON COLUMN cmx_code_rule.condition IS '适用条件（JSON 算子 {"eq":[...]} 或字符串 field==value，可选）';
COMMENT ON COLUMN cmx_code_rule.segments IS '段序列 JSON（auto 必填）';
COMMENT ON COLUMN cmx_code_rule.joiner IS '段间连接符（默认空串）';
COMMENT ON COLUMN cmx_code_rule.pattern IS '校验正则（可选，manual 兜底 + auto 结果校验）';
COMMENT ON COLUMN cmx_code_rule.enable_gap IS '是否启用断号补偿（连号域才开，默认关）';
COMMENT ON COLUMN cmx_code_rule.use_sequence IS '是否使用 PG SEQUENCE 兜底（极端高并发可选，默认关）';
COMMENT ON COLUMN cmx_code_rule.valid_from IS '规则版本化·生效起始日期';
COMMENT ON COLUMN cmx_code_rule.valid_to IS '规则版本化·生效结束日期';
COMMENT ON COLUMN cmx_code_rule.priority IS '多规则选优（取大，默认 100）';
COMMENT ON COLUMN cmx_code_rule.is_active IS '是否启用';
COMMENT ON COLUMN cmx_code_rule.domain_code IS '所属域编码（如 fi），空串=兼容存量/全局可见';
COMMENT ON COLUMN cmx_code_rule.application_code IS '所属应用编码（如 cmxfico）';
COMMENT ON COLUMN cmx_code_rule.module_code IS '所属模块编码（如 gl）';

-- ─────────────────────────────────────────────────────
-- 2. 编码断号表
-- ─────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS cmx_code_gap (
    id              BIGINT                  NOT NULL,
    prefix          VARCHAR(128)            NOT NULL,
    serial_val      BIGINT                  NOT NULL,
    width           INT4                    NOT NULL,
    create_time     TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    PRIMARY KEY (id)
);

-- 按前缀查断号（take_gap 取最小断号）
CREATE INDEX IF NOT EXISTS ix_cmx_code_gap_prefix ON cmx_code_gap (prefix, serial_val);

COMMENT ON TABLE cmx_code_gap IS '编码断号表（只存空缺，≠已分配；连号域 enable_gap=true 才启用）';
COMMENT ON COLUMN cmx_code_gap.id IS '主键ID（pk52）';
COMMENT ON COLUMN cmx_code_gap.prefix IS '断号所属前缀（如 FV20260804）';
COMMENT ON COLUMN cmx_code_gap.serial_val IS '断号流水值（如 8）';
COMMENT ON COLUMN cmx_code_gap.width IS '流水宽度（补零用）';
