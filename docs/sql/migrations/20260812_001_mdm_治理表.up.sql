-- =============================================
-- 迁移说明：MDM 治理表建表（整合原 20260804_001/20260805_001/20260811_001/20260812_001/20260812_002 五个迁移为一份完整建表脚本）
-- 影响表：cmx_mdm_activation, md_audit, md_xref, md_value_map, md_match_config, md_merge_record, md_match_scan, md_subscription, md_event_log
-- 操作类型：CREATE TABLE / CREATE INDEX / INSERT (seed)
-- 回滚方式：20260812_001_mdm_治理表.down.sql
-- =============================================

-- MDM 治理表（平台级，不走 compile）
-- 约定：无 FOREIGN KEY（关联字段+索引替代）；cmx_ 平台表主键 VARCHAR(64) snowflake；md_ 治理表主键 BIGINT（承接 cm_*.id）；时间戳 TIMESTAMPTZ
-- 注：cv_mdm_apply / cv_mdm_apply_line 是单据表（cv_*），列结构由单据元数据驱动（dataplatform_doc_meta_v1.json），
--     不在本迁移建表——后端 diff 引擎据元数据自动同步物理列。

-- ─────────────────────────────────────────────────────
-- 1. 激活映射配置（UI 配置器维护，激活器读取执行）—— cmx_ 平台表，主键 VARCHAR(64) snowflake
-- ─────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS cmx_mdm_activation (
    id              VARCHAR(64)  NOT NULL,
    activation_code VARCHAR(64)  NOT NULL,
    source_doc_type VARCHAR(64)  NOT NULL,
    cr_type         VARCHAR(16)  NOT NULL,
    target_dict     VARCHAR(64)  NOT NULL,
    target_table    VARCHAR(64)  NOT NULL,
    header_mapping  JSONB        NOT NULL DEFAULT '{}'::jsonb,
    line_mappings   JSONB                 DEFAULT '{}'::jsonb,
    code_rule_code  VARCHAR(64),
    subject_name_field VARCHAR(64),
    subject_code_field VARCHAR(64),
    header_groups   JSONB        NOT NULL DEFAULT '[]'::jsonb,
    is_active       BOOLEAN      NOT NULL DEFAULT TRUE,
    created_at      TIMESTAMPTZ  NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ  NOT NULL DEFAULT now(),
    PRIMARY KEY (id)
);
-- 幂等演进：CREATE TABLE IF NOT EXISTS 对「表已存在但为旧版结构」的库会整条跳过，
-- 导致新增列缺失、后续 COMMENT/索引引用报「column does not exist」。故显式补列，
-- 全新库上是 no-op（列已由 CREATE 建好），旧库上把缺列补齐，两种情况都幂等。
ALTER TABLE cmx_mdm_activation ADD COLUMN IF NOT EXISTS subject_name_field VARCHAR(64);
ALTER TABLE cmx_mdm_activation ADD COLUMN IF NOT EXISTS subject_code_field VARCHAR(64);
ALTER TABLE cmx_mdm_activation ADD COLUMN IF NOT EXISTS header_groups   JSONB       NOT NULL DEFAULT '[]'::jsonb;
ALTER TABLE cmx_mdm_activation ADD COLUMN IF NOT EXISTS is_active       BOOLEAN     NOT NULL DEFAULT TRUE;
ALTER TABLE cmx_mdm_activation ADD COLUMN IF NOT EXISTS created_at      TIMESTAMPTZ NOT NULL DEFAULT now();
ALTER TABLE cmx_mdm_activation ADD COLUMN IF NOT EXISTS updated_at      TIMESTAMPTZ NOT NULL DEFAULT now();
COMMENT ON TABLE  cmx_mdm_activation IS 'MDM 激活映射配置（单据→主数据），UI 配置器维护，激活器读取执行';
COMMENT ON COLUMN cmx_mdm_activation.id              IS '主键（snowflake，应用层生成）';
COMMENT ON COLUMN cmx_mdm_activation.activation_code IS '映射码（如 supplier_apply）';
COMMENT ON COLUMN cmx_mdm_activation.source_doc_type IS '来源单据类型（如 mdm_supplier_apply）';
COMMENT ON COLUMN cmx_mdm_activation.cr_type         IS '变更类型 create/update/merge/block/flag_delete';
COMMENT ON COLUMN cmx_mdm_activation.target_dict     IS '目标头字典码（如 supplier）';
COMMENT ON COLUMN cmx_mdm_activation.target_table    IS '目标头物理表名（如 cm_supplier，配置器选字典时从 dct/meta tableName 一并写入，激活器直接用）';
COMMENT ON COLUMN cmx_mdm_activation.header_mapping  IS '头映射 {单据字段:主数据列}';
COMMENT ON COLUMN cmx_mdm_activation.line_mappings   IS '明细映射 [{lineType,targetDict,targetTable,parentIdField,fields}]';
COMMENT ON COLUMN cmx_mdm_activation.code_rule_code  IS 'code 由哪个编码规则生成（新建时接 cmx-code）';
COMMENT ON COLUMN cmx_mdm_activation.subject_name_field IS '主体名字段来源（payload 内字段名，前端按此填 subject_name）';
COMMENT ON COLUMN cmx_mdm_activation.subject_code_field IS '主体编码字段来源（为空则由 codeRule 铸号）';
COMMENT ON COLUMN cmx_mdm_activation.header_groups   IS '头映射分组(UI 展示用,[{groupCode,groupName,fields:[源字段名]}]);激活器不读,header_mapping 落库仍扁平';
COMMENT ON COLUMN cmx_mdm_activation.is_active       IS '是否启用';
CREATE UNIQUE INDEX IF NOT EXISTS uk_cmx_mdm_activation_code     ON cmx_mdm_activation (activation_code);
CREATE        INDEX IF NOT EXISTS idx_cmx_mdm_activation_doctype ON cmx_mdm_activation (source_doc_type, cr_type);

-- ─────────────────────────────────────────────────────
-- 2. 主数据版本留痕（激活器写入）—— md_ 治理表，主键 BIGINT
-- ─────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS md_audit (
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
COMMENT ON TABLE  md_audit IS '主数据版本留痕（激活器写入）';
COMMENT ON COLUMN md_audit.id            IS '主键（应用层生成）';
COMMENT ON COLUMN md_audit.dict_code     IS 'cm_* 字典码';
COMMENT ON COLUMN md_audit.record_id     IS 'cm_*.id（无物理FK）';
COMMENT ON COLUMN md_audit.version       IS '激活版本号';
COMMENT ON COLUMN md_audit.action        IS 'create/update/freeze/merge/archive';
COMMENT ON COLUMN md_audit.source_cr_id  IS '触发此变更的 CR 单据 cv_mdm_apply.id';
COMMENT ON COLUMN md_audit.field         IS '变更字段（变更场景）';
COMMENT ON COLUMN md_audit.old_value     IS '旧值';
COMMENT ON COLUMN md_audit.new_value     IS '新值';
COMMENT ON COLUMN md_audit.operated_by   IS '操作人ID';
CREATE INDEX IF NOT EXISTS idx_md_audit_record ON md_audit (dict_code, record_id, version);

-- ─────────────────────────────────────────────────────
-- 3. 交叉引用（Key Mapping）
-- ─────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS md_xref (
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
COMMENT ON TABLE  md_xref IS '主数据交叉引用（Key Mapping）';
COMMENT ON COLUMN md_xref.id          IS '主键（应用层生成）';
COMMENT ON COLUMN md_xref.xref_status IS '引用状态 active/inactive';
CREATE UNIQUE INDEX IF NOT EXISTS uk_md_xref_src     ON md_xref (source_system, source_ref);
CREATE        INDEX IF NOT EXISTS idx_md_xref_record ON md_xref (dict_code, record_id);

-- ─────────────────────────────────────────────────────
-- 4. 值映射（Value Mapping）
-- ─────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS md_value_map (
    id        BIGINT       NOT NULL,
    field     VARCHAR(64)  NOT NULL,
    src_sys   VARCHAR(64)  NOT NULL,
    src_val   VARCHAR(128) NOT NULL,
    tgt_sys   VARCHAR(64)  NOT NULL,
    tgt_val   VARCHAR(128) NOT NULL,
    PRIMARY KEY (id)
);
COMMENT ON TABLE  md_value_map IS '主数据值映射（Value Mapping）';
COMMENT ON COLUMN md_value_map.id IS '主键（应用层生成）';

-- ─────────────────────────────────────────────────────
-- 5. 查重规则配置（查重界面内维护，find-duplicates 读取执行）
-- ─────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS md_match_config (
    id             BIGINT       NOT NULL,
    rule_name      VARCHAR(128) NOT NULL,
    dict_code      VARCHAR(64)  NOT NULL,
    target_table   VARCHAR(64)  NOT NULL,
    specs          JSONB        NOT NULL DEFAULT '[]'::jsonb,
    cluster_keys   JSONB        NOT NULL DEFAULT '[]'::jsonb,
    survive_fields JSONB        NOT NULL DEFAULT '[]'::jsonb,
    thresholds     JSONB,
    is_active      BOOLEAN      NOT NULL DEFAULT TRUE,
    created_at     TIMESTAMPTZ  NOT NULL DEFAULT now(),
    PRIMARY KEY (id)
);
COMMENT ON TABLE  md_match_config IS '查重规则配置（按字典维度），查重界面内维护，find-duplicates 读取执行';
COMMENT ON COLUMN md_match_config.id IS '主键（应用层生成）';
COMMENT ON COLUMN md_match_config.specs IS '比较字段 [{field,weight,kind:Exact|EditDistance}]';
CREATE UNIQUE INDEX IF NOT EXISTS uk_md_match_config_dict_rule ON md_match_config (dict_code, rule_name);
CREATE        INDEX IF NOT EXISTS idx_md_match_config_dict      ON md_match_config (dict_code);

-- Seed: supplier 默认查重规则（id 固定值 1，应用层 next_pk_id 不会冲突）
INSERT INTO md_match_config (id, rule_name, dict_code, target_table, specs, cluster_keys, survive_fields, thresholds, is_active)
SELECT 1, '供应商默认查重', 'supplier', 'cm_supplier',
       '[{"field":"credit_code","weight":40,"kind":"Exact"},{"field":"tax_no","weight":30,"kind":"Exact"},{"field":"name","weight":30,"kind":"EditDistance"}]'::jsonb,
       '["credit_code","tax_no","name"]'::jsonb,
       '["name","tax_no","credit_code","short_name","phone"]'::jsonb,
       '{"auto_merge":95,"review":80}'::jsonb,
       TRUE
WHERE NOT EXISTS (SELECT 1 FROM md_match_config WHERE dict_code = 'supplier' AND rule_name = '供应商默认查重');

-- ─────────────────────────────────────────────────────
-- 6. 合并事务记录（管家确认合并的载体；承载 survivorship_log 存活留痕 + 状态流转）
--    旧名 md_match_group，已于本整合迁移直接采用最终名 md_merge_record
-- ─────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS md_merge_record (
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
COMMENT ON TABLE  md_merge_record IS '合并事务记录（管家确认合并的载体；承载 survivorship_log 存活留痕 + 状态流转。与 md_match_scan 职责分离：scan=系统扫描的嫌疑重复，merge_record=确认执行的合并事务）';
COMMENT ON COLUMN md_merge_record.id               IS '主键（应用层生成）';
COMMENT ON COLUMN md_merge_record.dict_code        IS 'cm_* 字典码';
COMMENT ON COLUMN md_merge_record.group_key        IS '合并组业务键，如 merge:{master_id}';
COMMENT ON COLUMN md_merge_record.member_ids       IS '簇内记录 id 数组 [master_id, ...victim_ids]';
COMMENT ON COLUMN md_merge_record.master_id        IS '主记录 id（合并后保留的一方）';
COMMENT ON COLUMN md_merge_record.score            IS '合并时簇内最高匹配分（0-100）';
COMMENT ON COLUMN md_merge_record.decision         IS '裁决结果 AutoMerge/Review（查重阶段判定）';
COMMENT ON COLUMN md_merge_record.survivorship_log IS '存活留痕 JSONB {fields:[{field,from,value}],reparented:{明细表:[行id]}}';
COMMENT ON COLUMN md_merge_record.status           IS 'pending/reviewed/rejected/unmerged（待审/已合并/已驳回/已还原）';
CREATE INDEX IF NOT EXISTS idx_md_merge_record_dict ON md_merge_record (dict_code, status);

-- ─────────────────────────────────────────────────────
-- 7. 查重发现项（全库扫描结果载体，管家评审用；与 md_merge_record 职责分离）
-- ─────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS md_match_scan (
    id            BIGINT       NOT NULL,
    dict_code     VARCHAR(64)  NOT NULL,
    cluster_key   VARCHAR(255) NOT NULL,
    cluster_hash  VARCHAR(64)  NOT NULL,
    member_ids    JSONB        NOT NULL,
    member_count  SMALLINT     NOT NULL,
    max_score     SMALLINT     NOT NULL,
    status        VARCHAR(16)  NOT NULL DEFAULT 'pending',
    scaned_at     TIMESTAMPTZ  NOT NULL DEFAULT now(),
    resolved_at   TIMESTAMPTZ,
    resolved_by   BIGINT,
    created_at    TIMESTAMPTZ  NOT NULL DEFAULT now(),
    PRIMARY KEY (id)
);
COMMENT ON TABLE  md_match_scan IS '查重发现项（系统扫描出的重复簇，管家评审载体）';
COMMENT ON COLUMN md_match_scan.id            IS '主键（应用层生成）';
COMMENT ON COLUMN md_match_scan.cluster_key   IS '簇键标识，如 credit_code:C1';
COMMENT ON COLUMN md_match_scan.cluster_hash  IS 'member_ids 升序后 hash，去重用';
COMMENT ON COLUMN md_match_scan.member_ids    IS '簇内记录 id 数组 [id1,id2,...]';
COMMENT ON COLUMN md_match_scan.max_score     IS '簇内最高配对分';
COMMENT ON COLUMN md_match_scan.status        IS 'pending/resolved/ignored';
CREATE INDEX IF NOT EXISTS idx_md_match_scan_dict_status ON md_match_scan (dict_code, status);
CREATE INDEX IF NOT EXISTS idx_md_match_scan_hash        ON md_match_scan (dict_code, cluster_hash);

-- ─────────────────────────────────────────────────────
-- 8. 分发订阅
-- ─────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS md_subscription (
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
COMMENT ON COLUMN md_subscription.id      IS '主键（应用层生成）';
COMMENT ON COLUMN md_subscription.channel IS '通道 event/rest/batch';

-- ─────────────────────────────────────────────────────
-- 9. 分发事件日志（激活器激活成功时写入；主键 VARCHAR(64) snowflake，seq 为有序拉取列非主键）
-- ─────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS md_event_log (
    id          VARCHAR(64)  NOT NULL,
    seq         BIGSERIAL    NOT NULL,
    dict_code   VARCHAR(64)  NOT NULL,
    record_id   BIGINT       NOT NULL,
    event_type  VARCHAR(16)  NOT NULL,
    payload     JSONB        NOT NULL,
    emitted_at  TIMESTAMPTZ  NOT NULL DEFAULT now(),
    PRIMARY KEY (id)
);
COMMENT ON TABLE  md_event_log IS '分发事件日志（delta，消费者按 seq 拉取）';
COMMENT ON COLUMN md_event_log.id         IS '主键（snowflake，应用层生成，对齐全库主键惯例）';
COMMENT ON COLUMN md_event_log.seq        IS '有序拉取序列（DB 自增，非主键，供消费者 delta 排序）';
COMMENT ON COLUMN md_event_log.event_type IS 'created/updated/merged';
CREATE UNIQUE INDEX IF NOT EXISTS uk_md_event_log_seq   ON md_event_log (seq);
CREATE        INDEX IF NOT EXISTS idx_md_event_log_dict ON md_event_log (dict_code, seq);
