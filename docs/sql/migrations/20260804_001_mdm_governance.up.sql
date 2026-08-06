-- MDM 治理表(平台级,不走 compile)
-- 约定:无 FOREIGN KEY(关联字段+索引替代);cmx_ 平台表主键 VARCHAR(64) snowflake;md_ 治理表主键 BIGINT(承接 cm_*.id BIGINT 外键);时间戳 TIMESTAMPTZ

-- 1. 激活映射配置(UI 配置器维护,激活器读取执行)——cmx_ 平台表,主键 VARCHAR(64) snowflake
CREATE TABLE IF NOT EXISTS cmx_mdm_activation
(
    id              VARCHAR(64)  NOT NULL,
    activation_code VARCHAR(64)  NOT NULL,
    source_doc_type VARCHAR(64)  NOT NULL,
    cr_type         VARCHAR(16)  NOT NULL,
    target_dict     VARCHAR(64)  NOT NULL,
    target_table    VARCHAR(64)  NOT NULL,
    header_mapping  JSONB        NOT NULL DEFAULT '{}'::jsonb,
    line_mappings   JSONB                 DEFAULT '{}'::jsonb,
    code_rule_code  VARCHAR(64),
    is_active       BOOLEAN      NOT NULL DEFAULT TRUE,
    created_at      TIMESTAMPTZ  NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ  NOT NULL DEFAULT now(),
    PRIMARY KEY (id)
);
COMMENT ON TABLE  cmx_mdm_activation IS 'MDM 激活映射配置(单据→主数据),UI 配置器维护,激活器读取执行';
COMMENT ON COLUMN cmx_mdm_activation.id IS '主键(snowflake,应用层生成)';
COMMENT ON COLUMN cmx_mdm_activation.activation_code IS '映射码(如 supplier_apply)';
COMMENT ON COLUMN cmx_mdm_activation.source_doc_type IS '来源单据类型(如 mdm_supplier_apply)';
COMMENT ON COLUMN cmx_mdm_activation.cr_type         IS '变更类型 create/update/merge/block/flag_delete';
COMMENT ON COLUMN cmx_mdm_activation.target_dict     IS '目标头字典码(如 supplier)';
COMMENT ON COLUMN cmx_mdm_activation.target_table    IS '目标头物理表名(如 cm_supplier,配置器选字典时从 dct/meta tableName 一并写入,激活器直接用)';
COMMENT ON COLUMN cmx_mdm_activation.header_mapping  IS '头映射 {单据字段:主数据列}';
COMMENT ON COLUMN cmx_mdm_activation.line_mappings   IS '明细映射 [{lineType,targetDict,targetTable,parentIdField,fields}]';
COMMENT ON COLUMN cmx_mdm_activation.code_rule_code  IS 'code 由哪个编码规则生成(新建时,M8 接 cmx-code)';
COMMENT ON COLUMN cmx_mdm_activation.is_active       IS '是否启用';
CREATE UNIQUE INDEX IF NOT EXISTS uk_cmx_mdm_activation_code     ON cmx_mdm_activation (activation_code);
CREATE        INDEX IF NOT EXISTS idx_cmx_mdm_activation_doctype ON cmx_mdm_activation (source_doc_type, cr_type);

-- 2. 主数据版本留痕(激活器写入)——md_ 治理表,主键 BIGINT(承接 cm_*.id)
CREATE TABLE IF NOT EXISTS md_audit
(
    id            BIGINT       NOT NULL,
    dict_code     VARCHAR(64)  NOT NULL,
    record_id     BIGINT       NOT NULL,
    version       INT          NOT NULL,
    action        VARCHAR(16)  NOT NULL,
    source_cr_id  BIGINT,
    field         VARCHAR(64),
    old_value     JSONB,
    new_value     JSONB,
    operated_by   BIGINT       NOT NULL,
    operated_at   TIMESTAMPTZ  NOT NULL DEFAULT now(),
    PRIMARY KEY (id)
);
COMMENT ON TABLE  md_audit IS '主数据版本留痕(激活器写入)';
COMMENT ON COLUMN md_audit.id IS '主键(应用层生成)';
COMMENT ON COLUMN md_audit.dict_code    IS 'cm_* 字典码';
COMMENT ON COLUMN md_audit.record_id    IS 'cm_*.id(无物理FK)';
COMMENT ON COLUMN md_audit.version      IS '激活版本号';
COMMENT ON COLUMN md_audit.action       IS 'create/update/freeze/merge/archive';
COMMENT ON COLUMN md_audit.source_cr_id IS '触发此变更的 CR 单据 cv_mdm_apply.id';
COMMENT ON COLUMN md_audit.field        IS '变更字段(变更场景)';
COMMENT ON COLUMN md_audit.old_value    IS '旧值';
COMMENT ON COLUMN md_audit.new_value    IS '新值';
COMMENT ON COLUMN md_audit.operated_by  IS '操作人ID';
CREATE INDEX IF NOT EXISTS idx_md_audit_record ON md_audit (dict_code, record_id, version);

-- 3. 交叉引用(Key Mapping)
CREATE TABLE IF NOT EXISTS md_xref
(
    id             BIGINT       NOT NULL,
    dict_code      VARCHAR(64)  NOT NULL,
    record_id      BIGINT       NOT NULL,
    source_system  VARCHAR(64)  NOT NULL,
    source_ref     VARCHAR(128) NOT NULL,
    xref_status    VARCHAR(16)  NOT NULL DEFAULT 'active',
    confidence     SMALLINT     NOT NULL DEFAULT 50,
    created_at     TIMESTAMPTZ  NOT NULL DEFAULT now(),
    PRIMARY KEY (id)
);
COMMENT ON TABLE  md_xref IS '主数据交叉引用(Key Mapping)';
COMMENT ON COLUMN md_xref.id IS '主键(应用层生成)';
COMMENT ON COLUMN md_xref.xref_status IS '引用状态 active/inactive';
CREATE UNIQUE INDEX IF NOT EXISTS uk_md_xref_src     ON md_xref (source_system, source_ref);
CREATE        INDEX IF NOT EXISTS idx_md_xref_record ON md_xref (dict_code, record_id);

-- 4. 值映射(Value Mapping)
CREATE TABLE IF NOT EXISTS md_value_map
(
    id        BIGINT       NOT NULL,
    field     VARCHAR(64)  NOT NULL,
    src_sys   VARCHAR(64)  NOT NULL,
    src_val   VARCHAR(128) NOT NULL,
    tgt_sys   VARCHAR(64)  NOT NULL,
    tgt_val   VARCHAR(128) NOT NULL,
    PRIMARY KEY (id)
);
COMMENT ON TABLE md_value_map IS '主数据值映射(Value Mapping)';
COMMENT ON COLUMN md_value_map.id IS '主键(应用层生成)';

-- 5. 匹配组/存活裁决
CREATE TABLE IF NOT EXISTS md_match_group
(
    id               BIGINT       NOT NULL,
    dict_code        VARCHAR(64)  NOT NULL,
    group_key        VARCHAR(256) NOT NULL,
    member_ids       JSONB        NOT NULL,
    master_id        BIGINT,
    score            SMALLINT     NOT NULL,
    decision         VARCHAR(16)  NOT NULL,
    survivorship_log JSONB,
    status           VARCHAR(16)  NOT NULL DEFAULT 'pending',
    created_at       TIMESTAMPTZ  NOT NULL DEFAULT now(),
    PRIMARY KEY (id)
);
COMMENT ON TABLE  md_match_group IS '匹配组/存活裁决';
COMMENT ON COLUMN md_match_group.id IS '主键(应用层生成)';
COMMENT ON COLUMN md_match_group.status IS 'pending/auto_merged/reviewed/rejected';
CREATE INDEX IF NOT EXISTS idx_md_match_group_dict ON md_match_group (dict_code, status);

-- 6. 分发订阅
CREATE TABLE IF NOT EXISTS md_subscription
(
    id          BIGINT       NOT NULL,
    target_sys  VARCHAR(64)  NOT NULL,
    dict_code   VARCHAR(64)  NOT NULL,
    filter      JSONB,
    field_map   JSONB,
    channel     VARCHAR(16)  NOT NULL,
    active      BOOLEAN      NOT NULL DEFAULT TRUE,
    created_at  TIMESTAMPTZ  NOT NULL DEFAULT now(),
    PRIMARY KEY (id)
);
COMMENT ON TABLE  md_subscription IS '分发订阅配置';
COMMENT ON COLUMN md_subscription.id IS '主键(应用层生成)';
COMMENT ON COLUMN md_subscription.channel IS '通道 event/rest/batch';

-- 7. 分发事件日志(激活器激活成功时写入)——md_ 治理表,主键 VARCHAR(64) snowflake;seq 为有序拉取列(非主键)
CREATE TABLE IF NOT EXISTS md_event_log
(
    id          VARCHAR(64)  NOT NULL,
    seq         BIGSERIAL    NOT NULL,
    dict_code   VARCHAR(64)  NOT NULL,
    record_id   BIGINT       NOT NULL,
    event_type  VARCHAR(16)  NOT NULL,
    payload     JSONB        NOT NULL,
    emitted_at  TIMESTAMPTZ  NOT NULL DEFAULT now(),
    PRIMARY KEY (id)
);
COMMENT ON TABLE  md_event_log IS '分发事件日志(delta,消费者按 seq 拉取)';
COMMENT ON COLUMN md_event_log.id IS '主键(snowflake,应用层生成,对齐全库主键惯例)';
COMMENT ON COLUMN md_event_log.seq IS '有序拉取序列(DB 自增,非主键,供消费者 delta 排序)';
COMMENT ON COLUMN md_event_log.event_type IS 'created/updated/merged';
CREATE UNIQUE INDEX IF NOT EXISTS uk_md_event_log_seq   ON md_event_log (seq);
CREATE        INDEX IF NOT EXISTS idx_md_event_log_dict ON md_event_log (dict_code, seq);
