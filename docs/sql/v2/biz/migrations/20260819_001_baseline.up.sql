-- ============================================================
-- 迁移说明：业务库基线迁移 —— MDM 治理表 + 流程运行态表 + 单据版本化表 + 种子
-- 影响表：md_* 11 张治理表；cmx_flow_* 15 张流程运行态表；
--         cmx_doc_revision / cmx_doc_change 单据版本化 2 表；cr_report_sheet 索引
--         （cr_* 报表表 DDL 由模型中心部署）
-- 操作类型：CREATE TABLE/INDEX IF NOT EXISTS + 对齐 ALTER + INSERT ON CONFLICT（无损幂等）
-- 回滚方式：无独立 down（基线不做回滚）
-- 说明：本文件 = biz/init_ddl.sql + biz/init_dml.sql 合并
--       + cr_report_sheet 索引修正（原 20260720_001_cr_report_sheet_multisheet）。
--       流程表建业务库与流程引擎运行时一致（FLOW_DB_ID=业务库）；
--       存量环境若曾把 md_* 建在主库，见 docs/sql/v2/README.md 搬运指引。
--       单据版本化 2 表原在平台库基线，20260827 迁入；主库遗留旧表的
--       数据搬运 / 归档同见 README.md §五。
-- ============================================================

-- ============================================================
-- CMX 业务库全量 DDL（基线内嵌版）
--
-- 与 init_ddl.sql 的差异：每表区块内多一段「结构对齐 ALTER」
-- （存量库补列；新库空操作）。每表区块：CREATE TABLE → 结构对齐 ALTER → COMMENT → 索引
-- ============================================================


-- ================================================================
-- cmx-flow 流程引擎（M1 + M2）：运行态(RU) 3 表 + 历史态(HI) 2 表
-- 无 FOREIGN KEY（关联字段 + 索引替代）；实例终态时同事务归档到 HI 表。
-- 详见 docs/sql/migrations/20260717_001_cmx_flow_engine_tables.up.sql
-- ================================================================

-- 流程实例（运行态聚合根）
CREATE TABLE IF NOT EXISTS cmx_flow_instance
(
    id                  VARCHAR(64)  NOT NULL,
    definition_key      VARCHAR(128) NOT NULL,
    business_key        VARCHAR(128),
    state               VARCHAR(16)  NOT NULL,
    variables           JSONB        NOT NULL DEFAULT '{}'::jsonb,
    created_at          TIMESTAMPTZ  NOT NULL,
    updated_at          TIMESTAMPTZ  NOT NULL,
    ended_at            TIMESTAMPTZ,
    org_id              VARCHAR(64),
    parent_instance_id  VARCHAR(64),
    parent_token_id     VARCHAR(64),
    parent_node_bpmn_id VARCHAR(128),
    PRIMARY KEY (id)
);
COMMENT ON TABLE  cmx_flow_instance                    IS '流程实例（运行态聚合根）';
COMMENT ON COLUMN cmx_flow_instance.state              IS '实例状态：ACTIVE / COMPLETED / TERMINATED';
COMMENT ON COLUMN cmx_flow_instance.variables          IS '实例级流程变量（JSONB 动态 KV）';
COMMENT ON COLUMN cmx_flow_instance.parent_instance_id IS '父实例 id（M5 子流程：子实例指向主实例；主实例为 NULL）';
COMMENT ON COLUMN cmx_flow_instance.parent_token_id    IS '父实例中挂起等待的令牌 id（子完成时精确唤醒）';
COMMENT ON COLUMN cmx_flow_instance.parent_node_bpmn_id IS '父实例中发起本子实例的 callActivity 节点 bpmn id（M5.3 多挂载去重键；单挂载恒空）';
CREATE INDEX IF NOT EXISTS idx_cmx_flow_instance_defkey ON cmx_flow_instance (definition_key);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_instance_bizkey ON cmx_flow_instance (business_key);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_instance_state  ON cmx_flow_instance (state);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_instance_parent ON cmx_flow_instance (parent_instance_id);

-- 流程令牌（执行指针）
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
COMMENT ON TABLE  cmx_flow_token       IS '流程令牌（执行指针；一实例多令牌）';
COMMENT ON COLUMN cmx_flow_token.state IS '令牌状态：ACTIVE / WAITING / JOINING / WAITING_SUBFLOW / ENDED';
CREATE INDEX IF NOT EXISTS idx_cmx_flow_token_instance ON cmx_flow_token (instance_id);

-- 用户任务（等待态外化）
CREATE TABLE IF NOT EXISTS cmx_flow_task
(
    id               VARCHAR(64)  NOT NULL,
    instance_id      VARCHAR(64)  NOT NULL,
    token_id         VARCHAR(64)  NOT NULL,
    node_bpmn_id     VARCHAR(128) NOT NULL,
    name             VARCHAR(255),
    assignee         VARCHAR(128),
    candidate_groups VARCHAR(512),
    element_value    JSONB,
    owner_user_id    VARCHAR(64),
    parent_task_id   VARCHAR(64),
    delegation_state VARCHAR(16),
    completed        BOOLEAN      NOT NULL DEFAULT FALSE,
    created_at       TIMESTAMPTZ  NOT NULL,
    completed_at     TIMESTAMPTZ,
    PRIMARY KEY (id)
);
COMMENT ON TABLE  cmx_flow_task IS '用户任务（userTask 等待态的外化产物）';
COMMENT ON COLUMN cmx_flow_task.element_value    IS '多实例子任务携带的当前元素（会签每人各自数据；单实例为 NULL）';
COMMENT ON COLUMN cmx_flow_task.owner_user_id    IS 'M4.3 任务所有者（委派时 ≠ assignee）';
COMMENT ON COLUMN cmx_flow_task.parent_task_id   IS 'M4.3 父任务（加签临时任务指向原任务）';
COMMENT ON COLUMN cmx_flow_task.delegation_state IS 'M4.3 转签状态：NULL/DELEGATED/ADDSIGN/SUSPENDED';
CREATE INDEX IF NOT EXISTS idx_cmx_flow_task_instance ON cmx_flow_task (instance_id);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_task_assignee ON cmx_flow_task (assignee);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_task_open     ON cmx_flow_task (assignee, completed);

-- 历史实例（终态归档）
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
COMMENT ON COLUMN cmx_flow_hi_instance.duration_ms IS '实例存续时长（毫秒）';
CREATE INDEX IF NOT EXISTS idx_cmx_flow_hi_instance_defkey ON cmx_flow_hi_instance (definition_key);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_hi_instance_bizkey ON cmx_flow_hi_instance (business_key);

-- 历史任务（办结归档）
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
COMMENT ON COLUMN cmx_flow_hi_task.duration_ms IS '任务办理时长（毫秒）';
CREATE INDEX IF NOT EXISTS idx_cmx_flow_hi_task_instance ON cmx_flow_hi_task (instance_id);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_hi_task_assignee ON cmx_flow_hi_task (assignee);

-- 多实例执行域（M3：会签/或签账本；详见 migrations/20260717_002_cmx_flow_multi_instance.up.sql）
CREATE TABLE IF NOT EXISTS cmx_flow_mi_scope
(
    id                   VARCHAR(64)  NOT NULL,
    instance_id          VARCHAR(64)  NOT NULL,
    node_bpmn_id         VARCHAR(128) NOT NULL,
    sequential           BOOLEAN      NOT NULL DEFAULT FALSE,
    total                INTEGER      NOT NULL,
    completed            INTEGER      NOT NULL DEFAULT 0,
    next_index           INTEGER      NOT NULL DEFAULT 0,
    collection           JSONB        NOT NULL DEFAULT '[]'::jsonb,
    element_var          VARCHAR(128),
    completion_condition VARCHAR(512),
    finished             BOOLEAN      NOT NULL DEFAULT FALSE,
    PRIMARY KEY (id)
);
COMMENT ON TABLE  cmx_flow_mi_scope             IS '多实例执行域（会签/或签的一次展开：计数与游标账本）';
COMMENT ON COLUMN cmx_flow_mi_scope.sequential  IS 'true=顺序(或签)；false=并行(会签)';
COMMENT ON COLUMN cmx_flow_mi_scope.total       IS '子实例总数（nrOfInstances）';
COMMENT ON COLUMN cmx_flow_mi_scope.completed   IS '已办结子实例数（nrOfCompletedInstances）';
COMMENT ON COLUMN cmx_flow_mi_scope.finished    IS '本域是否已收口（完成条件命中或全部完成）';
CREATE INDEX IF NOT EXISTS idx_cmx_flow_mi_scope_instance ON cmx_flow_mi_scope (instance_id);

-- 定时器作业（M2.5：边界定时器到期表；详见 migrations/20260717_003_cmx_flow_job.up.sql）
CREATE TABLE IF NOT EXISTS cmx_flow_job
(
    id               VARCHAR(64)  NOT NULL,
    instance_id      VARCHAR(64)  NOT NULL,
    token_id         VARCHAR(64)  NOT NULL,
    boundary_bpmn_id VARCHAR(128) NOT NULL,
    cancel_activity  BOOLEAN      NOT NULL DEFAULT TRUE,
    due_at           TIMESTAMPTZ  NOT NULL,
    created_at       TIMESTAMPTZ  NOT NULL,
    PRIMARY KEY (id)
);
COMMENT ON TABLE  cmx_flow_job                 IS '定时器作业（边界定时器到期表）';
COMMENT ON COLUMN cmx_flow_job.token_id        IS '挂载令牌 id（停在宿主 userTask）；令牌离开即撤销作业';
COMMENT ON COLUMN cmx_flow_job.cancel_activity IS 'true=中断型；false=非中断型';
COMMENT ON COLUMN cmx_flow_job.due_at          IS '到期时刻（宿主到达 + 时长）';
CREATE INDEX IF NOT EXISTS idx_cmx_flow_job_instance ON cmx_flow_job (instance_id);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_job_due      ON cmx_flow_job (due_at);

-- 任务候选人池（M4.1：多人候选待认领）
CREATE TABLE IF NOT EXISTS cmx_flow_task_candidate
(
    id               VARCHAR(64)  NOT NULL,
    task_id          VARCHAR(64)  NOT NULL,
    instance_id      VARCHAR(64)  NOT NULL,
    candidate_type   VARCHAR(16)  NOT NULL,
    candidate_ref    VARCHAR(128) NOT NULL,
    resolved_user_id VARCHAR(64)  NOT NULL,
    PRIMARY KEY (id)
);
COMMENT ON TABLE  cmx_flow_task_candidate                IS '任务候选人池（多人候选待认领；随快照全删重插）';
COMMENT ON COLUMN cmx_flow_task_candidate.candidate_type IS '候选来源：USER / ROLE / POSITION / ORG';
CREATE INDEX IF NOT EXISTS idx_cmx_flow_task_candidate_instance ON cmx_flow_task_candidate (instance_id);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_task_candidate_user     ON cmx_flow_task_candidate (resolved_user_id);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_task_candidate_task     ON cmx_flow_task_candidate (task_id);

-- 抄送记录（M4.2：知会 + 已读；详见 migrations/20260718_002_cmx_flow_cc.up.sql）
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
COMMENT ON TABLE  cmx_flow_cc            IS '抄送记录表（只读知会 + 已读追踪；不阻塞流程）';
COMMENT ON COLUMN cmx_flow_cc.read_at    IS '已读时刻（NULL = 未读）';
CREATE INDEX IF NOT EXISTS idx_cmx_flow_cc_instance ON cmx_flow_cc (instance_id);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_cc_to_user  ON cmx_flow_cc (to_user_id, read_at);

-- 转签台账（M4.3：转办/加签/委派；详见 migrations/20260718_003_cmx_flow_delegation.up.sql）
CREATE TABLE IF NOT EXISTS cmx_flow_task_delegation
(
    id           VARCHAR(64)  NOT NULL,
    task_id      VARCHAR(64)  NOT NULL,
    instance_id  VARCHAR(64)  NOT NULL,
    kind         VARCHAR(20)  NOT NULL,
    from_user_id VARCHAR(64)  NOT NULL,
    to_user_id   VARCHAR(64)  NOT NULL,
    temp_task_id VARCHAR(64),
    reason       VARCHAR(500),
    created_at   TIMESTAMPTZ  NOT NULL,
    PRIMARY KEY (id)
);
COMMENT ON TABLE  cmx_flow_task_delegation      IS '转签台账（转办/加签/委派流转链）';
COMMENT ON COLUMN cmx_flow_task_delegation.kind IS 'TRANSFER / ADDSIGN_BEFORE / ADDSIGN_AFTER / DELEGATE';
CREATE INDEX IF NOT EXISTS idx_cmx_flow_task_delegation_instance ON cmx_flow_task_delegation (instance_id);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_task_delegation_task     ON cmx_flow_task_delegation (task_id);

-- 子流程组织绑定（M5.2：逻辑 key + 组织 → 具体子流程；详见 migrations/20260718_005_cmx_flow_subflow_binding.up.sql）
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
COMMENT ON TABLE  cmx_flow_subflow_binding       IS '子流程组织绑定（逻辑 key + 组织 → 具体子流程；M5.2 路由数据源）';
COMMENT ON COLUMN cmx_flow_subflow_binding.org_id IS '适用组织（NULL = 默认兜底绑定）';
CREATE INDEX IF NOT EXISTS idx_cmx_flow_subflow_binding_key ON cmx_flow_subflow_binding (called_key);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_subflow_binding_org ON cmx_flow_subflow_binding (org_id);

-- ================================================================
-- cmx-flow 流程定义持久化层（设计器 阶段0）
-- 含 DAM 三段列 + 版本变更说明列；详见 migrations/20260718_007/009/010
-- ================================================================

-- 流程定义主记录（当前指针：草稿 XML + 已发布版本指向）
CREATE TABLE IF NOT EXISTS cmx_flow_definition (
    key            VARCHAR(128) PRIMARY KEY,
    name           VARCHAR(255) NOT NULL,
    module         VARCHAR(64),
    category       VARCHAR(64),
    state          VARCHAR(16)  NOT NULL DEFAULT 'DRAFT',
    active_version INTEGER,
    draft_xml      TEXT,
    domain         VARCHAR(64),
    application    VARCHAR(64),
    updated_at     TIMESTAMPTZ  NOT NULL,
    updated_by     VARCHAR(64)
);
COMMENT ON TABLE  cmx_flow_definition                IS '流程定义主记录（当前指针：草稿 XML + 已发布版本指向）';
COMMENT ON COLUMN cmx_flow_definition.key            IS '流程定义 key（= BPMN process id）';
COMMENT ON COLUMN cmx_flow_definition.state          IS '状态：DRAFT / PUBLISHED';
COMMENT ON COLUMN cmx_flow_definition.active_version IS '当前已发布版本号（未发布为 NULL）';
COMMENT ON COLUMN cmx_flow_definition.draft_xml      IS '当前草稿的 BPMN XML（设计器产物）';
COMMENT ON COLUMN cmx_flow_definition.domain         IS '所属域（DAM 三段之一，如 fi）';
COMMENT ON COLUMN cmx_flow_definition.application    IS '所属应用（DAM 三段之一，如 cmxfico）';
COMMENT ON COLUMN cmx_flow_definition.module         IS '所属模块（DAM 三段之一，如 gl）';
CREATE INDEX IF NOT EXISTS idx_cmx_flow_definition_module ON cmx_flow_definition (module);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_definition_state  ON cmx_flow_definition (state);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_definition_dam    ON cmx_flow_definition (domain, application, module);

-- 流程定义版本历史（不可变，每次发布追加 BPMN 快照）
CREATE TABLE IF NOT EXISTS cmx_flow_definition_version (
    id           VARCHAR(64)  PRIMARY KEY,
    def_key      VARCHAR(128) NOT NULL,
    version      INTEGER      NOT NULL,
    bpmn_xml     TEXT         NOT NULL,
    note         VARCHAR(512),
    published_at TIMESTAMPTZ  NOT NULL,
    published_by VARCHAR(64)
);
COMMENT ON TABLE  cmx_flow_definition_version         IS '流程定义版本历史（不可变，每次发布追加 BPMN 快照）';
COMMENT ON COLUMN cmx_flow_definition_version.version IS '版本号（同 def_key 下从 1 递增）';
COMMENT ON COLUMN cmx_flow_definition_version.note    IS '本版本变更说明（发布时填写，可空）';
CREATE UNIQUE INDEX IF NOT EXISTS uq_cmx_flow_def_version ON cmx_flow_definition_version (def_key, version);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_def_version_key   ON cmx_flow_definition_version (def_key);

-- ================================================================
-- MDM 主数据治理表（平台级，不走 compile）
-- 含激活映射配置 / 版本留痕 / 交叉引用 / 值映射 / 匹配组 / 分发订阅 / 事件日志
-- 主键规约：cmx_ 平台表 VARCHAR(64) snowflake；md_ 治理表 BIGINT（承接 cm_*.id）
-- 无外键约束（关联字段 + 索引替代）
-- 详见 migrations/20260804_001_mdm_governance
-- ================================================================

-- 1. 激活映射配置（UI 配置器维护，激活器读取执行）
CREATE TABLE IF NOT EXISTS mdm_activation (
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
    doc_code_rules  JSONB        NOT NULL DEFAULT '{}'::jsonb,
    key_fields      JSONB        NOT NULL DEFAULT '[]'::jsonb,
    is_active       BOOLEAN      NOT NULL DEFAULT TRUE,
    created_at      TIMESTAMPTZ  NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ  NOT NULL DEFAULT now(),
    PRIMARY KEY (id)
);

-- —— 结构对齐（漂移库补列/索引重建；新库空操作） ——
ALTER TABLE mdm_activation ADD COLUMN IF NOT EXISTS subject_name_field VARCHAR(64);
ALTER TABLE mdm_activation ADD COLUMN IF NOT EXISTS subject_code_field VARCHAR(64);
ALTER TABLE mdm_activation ADD COLUMN IF NOT EXISTS header_groups   JSONB       NOT NULL DEFAULT '[]'::jsonb;
ALTER TABLE mdm_activation ADD COLUMN IF NOT EXISTS is_active       BOOLEAN     NOT NULL DEFAULT TRUE;
ALTER TABLE mdm_activation ADD COLUMN IF NOT EXISTS created_at      TIMESTAMPTZ NOT NULL DEFAULT now();
ALTER TABLE mdm_activation ADD COLUMN IF NOT EXISTS updated_at      TIMESTAMPTZ NOT NULL DEFAULT now();
COMMENT ON COLUMN mdm_activation.id              IS '主键（snowflake，应用层生成）';
COMMENT ON COLUMN mdm_activation.activation_code IS '映射码（如 supplier_apply）';
COMMENT ON COLUMN mdm_activation.source_doc_type IS '来源单据类型（如 mdm_supplier_apply）';
COMMENT ON COLUMN mdm_activation.cr_type         IS '变更类型 create/update/merge/block/flag_delete';
COMMENT ON COLUMN mdm_activation.target_dict     IS '目标头字典码（如 supplier）';
COMMENT ON COLUMN mdm_activation.target_table    IS '目标头物理表名（如 cm_supplier，配置器选字典时从 dct/meta tableName 一并写入，激活器直接用）';
COMMENT ON COLUMN mdm_activation.header_mapping  IS '头映射 {单据字段:主数据列}';
COMMENT ON COLUMN mdm_activation.line_mappings   IS '明细映射 [{lineType,targetDict,targetTable,parentIdField,fields}]';
COMMENT ON COLUMN mdm_activation.code_rule_code  IS 'code 由哪个编码规则生成（新建时接 cmx-code）';
COMMENT ON COLUMN mdm_activation.subject_name_field IS '主体名字段来源（payload 内字段名，前端按此填 subject_name）';
COMMENT ON COLUMN mdm_activation.subject_code_field IS '主体编码字段来源（为空则由 codeRule 铸号）';
COMMENT ON COLUMN mdm_activation.header_groups   IS '头映射分组(UI 展示用,[{groupCode,groupName,fields:[源字段名]}]);激活器不读,header_mapping 落库仍扁平';
COMMENT ON COLUMN mdm_activation.is_active       IS '是否启用';
ALTER TABLE mdm_activation ADD COLUMN IF NOT EXISTS doc_code_rules JSONB NOT NULL DEFAULT '{}'::jsonb;
COMMENT ON COLUMN mdm_activation.doc_code_rules IS '单据字段铸号规则覆盖 {单据字段:ruleCode}，单据保存铸号时覆盖单据元数据 codeRule 同名字段（激活配置优先）';;
ALTER TABLE mdm_activation ADD COLUMN IF NOT EXISTS key_fields JSONB NOT NULL DEFAULT '[]'::jsonb;
COMMENT ON COLUMN mdm_activation.key_fields IS '关键信息字段 [{field,weight,kind,dedup}];field=目标字典列名,数组序=簇键优先级;cr-form 据此渲染步骤①关键信息表单,dedup=true 的字段构造 /mdm/check-key 多字段加权查重,dedup=false 仅展示采集不查重;空则无步骤①(直接完整表单,不查重)';;
COMMENT ON TABLE  mdm_activation IS 'MDM 激活映射配置（单据→主数据），UI 配置器维护，激活器读取执行';
COMMENT ON COLUMN mdm_activation.id              IS '主键（snowflake，应用层生成）';
COMMENT ON COLUMN mdm_activation.activation_code IS '映射码（如 supplier_apply）';
COMMENT ON COLUMN mdm_activation.source_doc_type IS '来源单据类型（如 mdm_supplier_apply）';
COMMENT ON COLUMN mdm_activation.cr_type         IS '变更类型 create/update/merge/block/flag_delete';
COMMENT ON COLUMN mdm_activation.target_dict     IS '目标头字典码（如 supplier）';
COMMENT ON COLUMN mdm_activation.target_table    IS '目标头物理表名（如 cm_supplier，配置器选字典时从 dct/meta tableName 一并写入，激活器直接用）';
COMMENT ON COLUMN mdm_activation.header_mapping  IS '头映射 {单据字段:主数据列}';
COMMENT ON COLUMN mdm_activation.line_mappings   IS '明细映射 [{lineType,targetDict,targetTable,parentIdField,fields}]';
COMMENT ON COLUMN mdm_activation.code_rule_code  IS '【已废弃】字典 code 铸号规则；字典 code 现改走 dictMeta.codeRule，本列保留不删（避免迁移风险），激活器不再读取';
COMMENT ON COLUMN mdm_activation.subject_name_field IS '主体名字段来源（payload 内字段名，前端按此填 subject_name）';
COMMENT ON COLUMN mdm_activation.subject_code_field IS '【已废弃】主体编码字段来源；从未接线（激活器不读），字典 code 走 dictMeta.codeRule 铸号，本列保留不删（避免迁移风险）';
COMMENT ON COLUMN mdm_activation.header_groups  IS '头映射分组(UI 展示用,[{groupCode,groupName,fields:[源字段名]}]);激活器不读,header_mapping 落库仍扁平';
COMMENT ON COLUMN mdm_activation.doc_code_rules IS '单据字段铸号规则覆盖 {单据字段:ruleCode};单据保存铸号时覆盖单据元数据 codeRule 同名字段(激活配置优先);激活器不读,由 cr-form 读取经 saveDocData→saver 覆盖铸号';
COMMENT ON COLUMN mdm_activation.key_fields IS '关键信息字段 [{field,weight,kind,dedup}];field=目标字典列名,数组序=簇键优先级;cr-form 据此渲染步骤①关键信息表单,dedup=true 的字段构造 /mdm/check-key 多字段加权查重,dedup=false 仅展示采集不查重;空则无步骤①(直接完整表单,不查重)';
COMMENT ON COLUMN mdm_activation.is_active       IS '是否启用';
CREATE UNIQUE INDEX IF NOT EXISTS uk_mdm_activation_code     ON mdm_activation (activation_code);
CREATE        INDEX IF NOT EXISTS idx_mdm_activation_doctype ON mdm_activation (source_doc_type, cr_type);

-- 2. 主数据版本留痕（激活器写入）
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

-- —— 结构对齐（漂移库补列/索引重建；新库空操作） ——
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

-- 3. 交叉引用（Key Mapping）
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

-- —— 结构对齐（漂移库补列/索引重建；新库空操作） ——
COMMENT ON COLUMN md_xref.id          IS '主键（应用层生成）';
COMMENT ON COLUMN md_xref.xref_status IS '引用状态 active/inactive';
COMMENT ON TABLE  md_xref IS '主数据交叉引用（Key Mapping）';
COMMENT ON COLUMN md_xref.id          IS '主键（应用层生成）';
COMMENT ON COLUMN md_xref.xref_status IS '引用状态 active/inactive';
CREATE UNIQUE INDEX IF NOT EXISTS uk_md_xref_src     ON md_xref (source_system, source_ref);
CREATE        INDEX IF NOT EXISTS idx_md_xref_record ON md_xref (dict_code, record_id);

-- 4. 值映射（Value Mapping）
CREATE TABLE IF NOT EXISTS md_value_map (
    id        BIGINT       NOT NULL,
    field     VARCHAR(64)  NOT NULL,
    src_sys   VARCHAR(64)  NOT NULL,
    src_val   VARCHAR(128) NOT NULL,
    tgt_sys   VARCHAR(64)  NOT NULL,
    tgt_val   VARCHAR(128) NOT NULL,
    PRIMARY KEY (id)
);

-- —— 结构对齐（漂移库补列/索引重建；新库空操作） ——
COMMENT ON COLUMN md_value_map.id IS '主键（应用层生成）';
COMMENT ON TABLE  md_value_map IS '主数据值映射（Value Mapping）';
COMMENT ON COLUMN md_value_map.id IS '主键（应用层生成）';

-- 5. 查重规则配置（查重界面内维护，find-duplicates 读取执行）
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

-- —— 结构对齐（漂移库补列/索引重建；新库空操作） ——
COMMENT ON COLUMN md_match_config.id IS '主键（应用层生成）';
COMMENT ON COLUMN md_match_config.specs IS '比较字段 [{field,weight,kind:Exact|EditDistance}]';
COMMENT ON TABLE  md_match_config IS '查重规则配置（按字典维度），查重界面内维护，find-duplicates 读取执行';
COMMENT ON COLUMN md_match_config.id IS '主键（应用层生成）';
COMMENT ON COLUMN md_match_config.specs IS '比较字段 [{field,weight,kind:Exact|EditDistance}]';
CREATE UNIQUE INDEX IF NOT EXISTS uk_md_match_config_dict_rule ON md_match_config (dict_code, rule_name);
CREATE        INDEX IF NOT EXISTS idx_md_match_config_dict      ON md_match_config (dict_code);

-- 6. 匹配组/存活裁决
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

-- —— 结构对齐（漂移库补列/索引重建；新库空操作） ——
COMMENT ON COLUMN md_merge_record.id               IS '主键（应用层生成）';
COMMENT ON COLUMN md_merge_record.dict_code        IS 'cm_* 字典码';
COMMENT ON COLUMN md_merge_record.group_key        IS '合并组业务键，如 merge:{master_id}';
COMMENT ON COLUMN md_merge_record.member_ids       IS '簇内记录 id 数组 [master_id, ...victim_ids]';
COMMENT ON COLUMN md_merge_record.master_id        IS '主记录 id（合并后保留的一方）';
COMMENT ON COLUMN md_merge_record.score            IS '合并时簇内最高匹配分（0-100）';
COMMENT ON COLUMN md_merge_record.decision         IS '裁决结果 AutoMerge/Review（查重阶段判定）';
COMMENT ON COLUMN md_merge_record.survivorship_log IS '存活留痕 JSONB {fields:[{field,from,value}],reparented:{明细表:[行id]}}';
COMMENT ON COLUMN md_merge_record.status           IS 'pending/reviewed/rejected/unmerged（待审/已合并/已驳回/已还原）';
COMMENT ON TABLE  md_merge_record IS '合并事务记录（管家确认合并的载体；承载 survivorship_log 存活留痕 + 状态流转。与 md_match_scan 职责分离：scan=系统扫描的嫌疑重复，group=确认执行的合并事务）';
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

-- 7. 查重发现项（全库扫描结果载体，管家评审用；与 md_merge_record 职责分离）
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

-- —— 结构对齐（漂移库补列/索引重建；新库空操作） ——
COMMENT ON COLUMN md_match_scan.id            IS '主键（应用层生成）';
COMMENT ON COLUMN md_match_scan.cluster_key   IS '簇键标识，如 credit_code:C1';
COMMENT ON COLUMN md_match_scan.cluster_hash  IS 'member_ids 升序后 hash，去重用';
COMMENT ON COLUMN md_match_scan.member_ids    IS '簇内记录 id 数组 [id1,id2,...]';
COMMENT ON COLUMN md_match_scan.max_score     IS '簇内最高配对分';
COMMENT ON COLUMN md_match_scan.status        IS 'pending/resolved/ignored';
COMMENT ON TABLE  md_match_scan IS '查重发现项（系统扫描出的重复簇，管家评审载体）';
COMMENT ON COLUMN md_match_scan.id            IS '主键（应用层生成）';
COMMENT ON COLUMN md_match_scan.cluster_key   IS '簇键标识，如 credit_code:C1';
COMMENT ON COLUMN md_match_scan.cluster_hash  IS 'member_ids 升序后 hash，去重用';
COMMENT ON COLUMN md_match_scan.member_ids    IS '簇内记录 id 数组 [id1,id2,...]';
COMMENT ON COLUMN md_match_scan.max_score     IS '簇内最高配对分';
COMMENT ON COLUMN md_match_scan.status        IS 'pending/resolved/ignored';
CREATE INDEX IF NOT EXISTS idx_md_match_scan_dict_status ON md_match_scan (dict_code, status);
CREATE INDEX IF NOT EXISTS idx_md_match_scan_hash        ON md_match_scan (dict_code, cluster_hash);

-- 8. 分发订阅
CREATE TABLE IF NOT EXISTS md_subscription (
    id             BIGINT       NOT NULL,
    target_sys     VARCHAR(64)  NOT NULL,
    dict_code      VARCHAR(64)  NOT NULL,
    filter         JSONB,
    field_map      JSONB,
    channel        VARCHAR(16)  NOT NULL,
    active         BOOLEAN      NOT NULL DEFAULT TRUE,
    name           VARCHAR(128),
    description    VARCHAR(512),
    channel_config JSONB        NOT NULL DEFAULT '{}',
    event_types    JSONB        NOT NULL DEFAULT '[]',
    retry_max      INT          NOT NULL DEFAULT 8,
    timeout_ms     INT          NOT NULL DEFAULT 10000,
    batch_size     INT          NOT NULL DEFAULT 50,
    created_by     VARCHAR(64),
    created_at     TIMESTAMPTZ  NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ  NOT NULL DEFAULT now(),
    PRIMARY KEY (id)
);

-- —— 结构对齐（漂移库补列/索引重建；新库空操作） ——
COMMENT ON COLUMN md_subscription.id      IS '主键（应用层生成）';
COMMENT ON COLUMN md_subscription.channel IS '通道 event/rest/batch';
ALTER TABLE md_subscription ADD COLUMN IF NOT EXISTS name           VARCHAR(128), ADD COLUMN IF NOT EXISTS description    VARCHAR(512), ADD COLUMN IF NOT EXISTS channel_config JSONB        NOT NULL DEFAULT '{}', ADD COLUMN IF NOT EXISTS event_types    JSONB        NOT NULL DEFAULT '[]', ADD COLUMN IF NOT EXISTS retry_max      INT          NOT NULL DEFAULT 8, ADD COLUMN IF NOT EXISTS timeout_ms     INT          NOT NULL DEFAULT 10000, ADD COLUMN IF NOT EXISTS batch_size     INT          NOT NULL DEFAULT 50, ADD COLUMN IF NOT EXISTS created_by     VARCHAR(64), ADD COLUMN IF NOT EXISTS updated_at     TIMESTAMPTZ  NOT NULL DEFAULT now();
COMMENT ON COLUMN md_subscription.name           IS '订阅名称（展示）';
COMMENT ON COLUMN md_subscription.channel_config IS '通道配置：webhook {url,secret,headers{}}；rest_pull {consumerId}；kafka {brokers,topic,partition_key}（骨架）';
COMMENT ON COLUMN md_subscription.event_types    IS '订阅事件类型 JSON 数组；[] = 全部(created/updated/merged)';
COMMENT ON COLUMN md_subscription.retry_max     IS '最大尝试次数（含首发）';
COMMENT ON COLUMN md_subscription.timeout_ms    IS '单次投递超时（毫秒）';
COMMENT ON COLUMN md_subscription.batch_size    IS '单轮该订阅最大投递数';
COMMENT ON COLUMN md_subscription.created_by    IS '创建人用户 id';
COMMENT ON COLUMN md_subscription.updated_at    IS '最近更新时间';
COMMENT ON COLUMN md_subscription.channel       IS '通道 webhook/kafka/rocketmq/rest_pull';
COMMENT ON TABLE  md_subscription IS '分发订阅配置';
COMMENT ON COLUMN md_subscription.id             IS '主键（应用层生成）';
COMMENT ON COLUMN md_subscription.target_sys     IS '目标系统标识（uk：同系统同字典同通道唯一）';
COMMENT ON COLUMN md_subscription.filter         IS '行级过滤条件 {conditions:[{field,op,value}],logic:"and"}';
COMMENT ON COLUMN md_subscription.field_map      IS '列级转换 {include:[],rename:{},mask:[]}（value_map 预留）';
COMMENT ON COLUMN md_subscription.channel        IS '通道 webhook/kafka/rocketmq/rest_pull';
COMMENT ON COLUMN md_subscription.name           IS '订阅名称（展示）';
COMMENT ON COLUMN md_subscription.description    IS '订阅描述';
COMMENT ON COLUMN md_subscription.channel_config IS '通道配置：webhook {url,secret,headers{}}；rest_pull {consumerId}；kafka {brokers,topic,partition_key}（骨架）';
COMMENT ON COLUMN md_subscription.event_types    IS '订阅事件类型 JSON 数组；[] = 全部(created/updated/merged)';
COMMENT ON COLUMN md_subscription.retry_max      IS '最大尝试次数（含首发）';
COMMENT ON COLUMN md_subscription.timeout_ms     IS '单次投递超时（毫秒）';
COMMENT ON COLUMN md_subscription.batch_size     IS '单轮该订阅最大投递数';
COMMENT ON COLUMN md_subscription.created_by     IS '创建人用户 id';
COMMENT ON COLUMN md_subscription.updated_at     IS '最近更新时间';
CREATE UNIQUE INDEX IF NOT EXISTS uk_md_subscription ON md_subscription (target_sys, dict_code, channel);

-- 9. 分发事件日志（激活器激活成功时写入；主键 VARCHAR(64) snowflake，seq 为有序拉取列非主键）
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

-- —— 结构对齐（漂移库补列/索引重建；新库空操作） ——
COMMENT ON COLUMN md_event_log.id         IS '主键（snowflake，应用层生成，对齐全库主键惯例）';
COMMENT ON COLUMN md_event_log.seq        IS '有序拉取序列（DB 自增，非主键，供消费者 delta 排序）';
COMMENT ON COLUMN md_event_log.event_type IS 'created/updated/merged';
COMMENT ON TABLE  md_event_log IS '分发事件日志（delta，消费者按 seq 拉取）';
COMMENT ON COLUMN md_event_log.id         IS '主键（snowflake，应用层生成，对齐全库主键惯例）';
COMMENT ON COLUMN md_event_log.seq        IS '有序拉取序列（DB 自增，非主键，供消费者 delta 排序）';
COMMENT ON COLUMN md_event_log.event_type IS 'created/updated/merged';
CREATE UNIQUE INDEX IF NOT EXISTS uk_md_event_log_seq   ON md_event_log (seq);
CREATE        INDEX IF NOT EXISTS idx_md_event_log_dict ON md_event_log (dict_code, seq);

-- 9a. 分发投递实例（事件×订阅：队列状态机 + 投递流水，M5 分发引擎载体）
CREATE TABLE IF NOT EXISTS md_dispatch_log (
    id               BIGINT       NOT NULL,
    subscription_id  BIGINT       NOT NULL,
    event_id         VARCHAR(64)  NOT NULL,
    event_seq        BIGINT       NOT NULL,
    dict_code        VARCHAR(64)  NOT NULL,
    record_id        BIGINT       NOT NULL,
    status           VARCHAR(16)  NOT NULL,
    attempts         INT          NOT NULL DEFAULT 0,
    next_retry_at    TIMESTAMPTZ,
    last_error       TEXT,
    http_status      INT,
    response_snippet VARCHAR(512),
    delivered_at     TIMESTAMPTZ,
    created_at       TIMESTAMPTZ  NOT NULL DEFAULT now(),
    updated_at       TIMESTAMPTZ  NOT NULL DEFAULT now(),
    PRIMARY KEY (id)
);

-- —— 结构对齐（漂移库补列/索引重建；新库空操作） ——
COMMENT ON COLUMN md_dispatch_log.id IS '主键（应用层 snowflake，对齐 md_ 治理表惯例）';
COMMENT ON COLUMN md_dispatch_log.subscription_id IS '订阅 id → md_subscription.id';
COMMENT ON COLUMN md_dispatch_log.event_id IS '事件 id → md_event_log.id（幂等键之一）';
COMMENT ON COLUMN md_dispatch_log.event_seq IS '事件序号 → md_event_log.seq（排序/诊断冗余）';
COMMENT ON COLUMN md_dispatch_log.dict_code IS '字典代码（冗余，过滤用）';
COMMENT ON COLUMN md_dispatch_log.record_id IS '主数据记录 id';
COMMENT ON COLUMN md_dispatch_log.status IS 'pending待投/running投递中/delivered成功/failed待重试/dead死信/skipped人工跳过';
COMMENT ON COLUMN md_dispatch_log.attempts IS '已尝试次数';
COMMENT ON COLUMN md_dispatch_log.next_retry_at IS 'failed 的下次可抢占时间（指数退避）；NULL=非 failed';
COMMENT ON COLUMN md_dispatch_log.last_error IS '最近一次错误信息';
COMMENT ON COLUMN md_dispatch_log.http_status IS 'webhook 响应码';
COMMENT ON COLUMN md_dispatch_log.response_snippet IS '响应体摘要（截断 512）';
COMMENT ON COLUMN md_dispatch_log.delivered_at IS '投递成功时间';
COMMENT ON COLUMN md_dispatch_log.created_at IS '创建时间';
COMMENT ON COLUMN md_dispatch_log.updated_at IS '最近状态变更时间';
COMMENT ON TABLE  md_dispatch_log IS '分发投递实例（事件×订阅）：队列状态机 + 投递流水';
COMMENT ON COLUMN md_dispatch_log.id IS '主键（应用层 snowflake，对齐 md_ 治理表惯例）';
COMMENT ON COLUMN md_dispatch_log.subscription_id IS '订阅 id → md_subscription.id';
COMMENT ON COLUMN md_dispatch_log.event_id IS '事件 id → md_event_log.id（幂等键之一）';
COMMENT ON COLUMN md_dispatch_log.event_seq IS '事件序号 → md_event_log.seq（排序/诊断冗余）';
COMMENT ON COLUMN md_dispatch_log.dict_code IS '字典代码（冗余，过滤用）';
COMMENT ON COLUMN md_dispatch_log.record_id IS '主数据记录 id';
COMMENT ON COLUMN md_dispatch_log.status IS 'pending待投/running投递中/delivered成功/failed待重试/dead死信/skipped人工跳过';
COMMENT ON COLUMN md_dispatch_log.attempts IS '已尝试次数';
COMMENT ON COLUMN md_dispatch_log.next_retry_at IS 'failed 的下次可抢占时间（指数退避）；NULL=非 failed';
COMMENT ON COLUMN md_dispatch_log.last_error IS '最近一次错误信息';
COMMENT ON COLUMN md_dispatch_log.http_status IS 'webhook 响应码';
COMMENT ON COLUMN md_dispatch_log.response_snippet IS '响应体摘要（截断 512）';
COMMENT ON COLUMN md_dispatch_log.delivered_at IS '投递成功时间';
COMMENT ON COLUMN md_dispatch_log.created_at IS '创建时间';
COMMENT ON COLUMN md_dispatch_log.updated_at IS '最近状态变更时间';
CREATE UNIQUE INDEX IF NOT EXISTS uk_md_dispatch_sub_event ON md_dispatch_log (subscription_id, event_id);
CREATE INDEX IF NOT EXISTS idx_md_dispatch_due   ON md_dispatch_log (status, next_retry_at);
CREATE INDEX IF NOT EXISTS idx_md_dispatch_sub   ON md_dispatch_log (subscription_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_md_dispatch_event ON md_dispatch_log (event_id);

-- 9b. 分发引擎扇出水位（全局单行 fanout）
CREATE TABLE IF NOT EXISTS md_dist_watermark (
    key        VARCHAR(32) NOT NULL,
    last_seq   BIGINT      NOT NULL DEFAULT 0,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (key)
);

-- —— 结构对齐（漂移库补列/索引重建；新库空操作） ——
COMMENT ON COLUMN md_dist_watermark.key IS '水位键（当前仅 fanout）';
COMMENT ON COLUMN md_dist_watermark.last_seq IS '已扇出处理的 md_event_log 最大 seq（无论是否命中订阅）';
COMMENT ON COLUMN md_dist_watermark.updated_at IS '最近推进时间';
COMMENT ON TABLE  md_dist_watermark IS '分发引擎扇出水位（全局单行 fanout）';
COMMENT ON COLUMN md_dist_watermark.key IS '水位键（当前仅 fanout）';
COMMENT ON COLUMN md_dist_watermark.last_seq IS '已扇出处理的 md_event_log 最大 seq（无论是否命中订阅）';
COMMENT ON COLUMN md_dist_watermark.updated_at IS '最近推进时间';

-- 9c. pull 消费者游标登记（监控/对账用；消费端仍应自持 seq）
CREATE TABLE IF NOT EXISTS md_consumer_offset (
    id          BIGINT      NOT NULL,
    consumer_id VARCHAR(64) NOT NULL,
    dict_code   VARCHAR(64) NOT NULL,
    acked_seq   BIGINT      NOT NULL DEFAULT 0,
    acked_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (id)
);

-- —— 结构对齐（漂移库补列/索引重建；新库空操作） ——
COMMENT ON COLUMN md_consumer_offset.id IS '主键（应用层 snowflake）';
COMMENT ON COLUMN md_consumer_offset.consumer_id IS '下游消费者标识（建议 = target_sys）';
COMMENT ON COLUMN md_consumer_offset.dict_code IS '字典代码';
COMMENT ON COLUMN md_consumer_offset.acked_seq IS '已确认消费到的 seq';
COMMENT ON COLUMN md_consumer_offset.acked_at IS '最近确认时间';
COMMENT ON TABLE  md_consumer_offset IS 'pull 消费者游标登记（监控/对账用；消费端仍应自持 seq）';
COMMENT ON COLUMN md_consumer_offset.id IS '主键（应用层 snowflake）';
COMMENT ON COLUMN md_consumer_offset.consumer_id IS '下游消费者标识（建议 = target_sys）';
COMMENT ON COLUMN md_consumer_offset.dict_code IS '字典代码';
COMMENT ON COLUMN md_consumer_offset.acked_seq IS '已确认消费到的 seq';
COMMENT ON COLUMN md_consumer_offset.acked_at IS '最近确认时间';
CREATE UNIQUE INDEX IF NOT EXISTS uk_md_consumer_offset ON md_consumer_offset (consumer_id, dict_code);

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

-- ─────────────────────────────────────────────────────
-- 3. 编码发号序列表
-- ─────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS cmx_code_seq (
    id              BIGINT                  NOT NULL,
    rule_code       VARCHAR(64)             NOT NULL,
    prefix          VARCHAR(128)            NOT NULL,
    current_val     BIGINT                  NOT NULL DEFAULT 0,
    width           INT4                    NOT NULL DEFAULT 4,
    update_time     TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    PRIMARY KEY (id)
);

CREATE UNIQUE INDEX IF NOT EXISTS uk_cmx_code_seq_prefix ON cmx_code_seq (rule_code, prefix);

COMMENT ON TABLE cmx_code_seq IS '编码发号序列表（集群安全发号源，use_sequence=true 才启用）';
COMMENT ON COLUMN cmx_code_seq.id IS '主键ID（pk52）';
COMMENT ON COLUMN cmx_code_seq.rule_code IS '关联 cmx_code_rule.rule_code';
COMMENT ON COLUMN cmx_code_seq.prefix IS '发号分组键（含 reset_key，如 FV20260804）';
COMMENT ON COLUMN cmx_code_seq.current_val IS '已发到的最大流水值（0=首启未探测）';
COMMENT ON COLUMN cmx_code_seq.width IS '流水宽度（补零用，记录首次发号时的宽度）';

-- ============================================================
-- 补丁段：旧 init_ddl 快照未收录的终态表（对象覆盖核对发现）
-- ============================================================

-- 单据↔流程实例关联 + 任务意见留痕（原迁移 20260801_001_cmx_flow_biz_link）
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

-- ============================================================
-- 业务单据版本化（DOC 单据版本审计）：整单快照 + 字段级变更明细
-- 原在平台库基线 42/43 号区块，20260827 迁入业务库。
-- append-only 审计表，此前从未建在业务库（无历史对齐需求）；
-- 运行时 cmx-model cmx-doc-store-pg DocRevision 写入。
-- ============================================================

-- 整单 JSONB 快照表（同 root 仅一行为当前版 is_current=1）
CREATE TABLE IF NOT EXISTS cmx_doc_revision
(
    id             BIGINT       NOT NULL,
    doc_file       VARCHAR(200) NOT NULL,
    root_table     VARCHAR(100) NOT NULL,
    root_id        VARCHAR(64)  NOT NULL,
    rev_no         INT4         NOT NULL,
    is_current     INT4         NOT NULL DEFAULT 1,
    op             VARCHAR(16),
    snapshot       JSONB        NOT NULL,
    change_summary JSONB,
    reason         VARCHAR(500),
    actor_id       VARCHAR(64),
    actor_name     VARCHAR(100),
    biz_status     VARCHAR(32),
    created_at     TIMESTAMP    DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (id)
);

COMMENT ON TABLE  cmx_doc_revision            IS '业务单据版本化：整单 JSONB 快照（append-only，方案 §6A）';
COMMENT ON COLUMN cmx_doc_revision.id          IS '版本记录主键（雪花）';
COMMENT ON COLUMN cmx_doc_revision.doc_file    IS '单据定义（哪种单据）';
COMMENT ON COLUMN cmx_doc_revision.root_table  IS '根层表名（如 cv_batch）';
COMMENT ON COLUMN cmx_doc_revision.root_id     IS '单据根行 id（字符串化）';
COMMENT ON COLUMN cmx_doc_revision.rev_no      IS '该单第几版（1,2,3...）';
COMMENT ON COLUMN cmx_doc_revision.is_current  IS '是否当前版（同 root 仅一行为 1）';
COMMENT ON COLUMN cmx_doc_revision.op          IS '操作: create/update/delete/restore';
COMMENT ON COLUMN cmx_doc_revision.snapshot    IS '整单列式包快照（前端 fromJSON 可直接还原）';
COMMENT ON COLUMN cmx_doc_revision.change_summary IS '本版变更摘要';
COMMENT ON COLUMN cmx_doc_revision.reason      IS '变更原因（reason_required 时必填）';
COMMENT ON COLUMN cmx_doc_revision.actor_id    IS '操作者 id';
COMMENT ON COLUMN cmx_doc_revision.actor_name  IS '操作者名';
COMMENT ON COLUMN cmx_doc_revision.biz_status  IS '冗余当时单据状态，便于按态检索';
COMMENT ON COLUMN cmx_doc_revision.created_at  IS '创建时间';

CREATE UNIQUE INDEX IF NOT EXISTS uk_doc_rev     ON cmx_doc_revision (doc_file, root_id, rev_no);
CREATE INDEX IF NOT EXISTS        idx_doc_rev_cur ON cmx_doc_revision (doc_file, root_id, is_current);
CREATE INDEX IF NOT EXISTS        idx_doc_rev_time ON cmx_doc_revision (root_id, created_at);

-- 字段级变更明细表（U 时逐字段一行，审计用）
CREATE TABLE IF NOT EXISTS cmx_doc_change
(
    id         BIGINT       NOT NULL,
    rev_id     BIGINT       NOT NULL,
    root_id    VARCHAR(64)  NOT NULL,
    layer      VARCHAR(100),
    row_id     VARCHAR(64),
    op         VARCHAR(8),
    field      VARCHAR(100),
    old_value  JSONB,
    new_value  JSONB,
    actor_id   VARCHAR(64),
    created_at TIMESTAMP    DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (id)
);

COMMENT ON TABLE  cmx_doc_change       IS '业务单据字段级变更明细（审计，方案 §6A.3）';
COMMENT ON COLUMN cmx_doc_change.id     IS '主键ID（雪花）';
COMMENT ON COLUMN cmx_doc_change.rev_id  IS '所属版本 cmx_doc_revision.id';
COMMENT ON COLUMN cmx_doc_change.root_id IS '单据根行 id（字符串化）';
COMMENT ON COLUMN cmx_doc_change.layer   IS '层表名';
COMMENT ON COLUMN cmx_doc_change.row_id  IS '变更的行';
COMMENT ON COLUMN cmx_doc_change.op      IS 'I/U/D';
COMMENT ON COLUMN cmx_doc_change.field   IS '变更字段（U 时逐字段一行）';
COMMENT ON COLUMN cmx_doc_change.old_value IS '旧值';
COMMENT ON COLUMN cmx_doc_change.new_value IS '新值';
COMMENT ON COLUMN cmx_doc_change.actor_id  IS '操作者 id';
COMMENT ON COLUMN cmx_doc_change.created_at IS '创建时间';

CREATE INDEX IF NOT EXISTS idx_doc_change_rev ON cmx_doc_change (rev_id);
CREATE INDEX IF NOT EXISTS idx_doc_change_row ON cmx_doc_change (root_id, row_id, field);

-- ============================================================
-- CMX 业务库内置数据（DML）— docs/sql/v2/biz/init_dml.sql
--
-- 目标库：业务数据源（source_type = "biz"）
-- 内容：MDM 治理种子（激活映射 + 编码规则 + 查重规则 + 分发水位）
-- 风格：无损幂等，全部 ON CONFLICT / NOT EXISTS 防重，可重复执行
-- 来源：迁移 20260818_001（激活映射/编码规则/查重）+ 20260812_001 + 20260813_002
-- ============================================================

-- ============================================================
-- 1. MDM 激活映射（mdm_activation）
-- 来源：迁移 20260818_001 段2（10 新域 × create/update 26 条）
--       + 段3（kh/wl/kj 深化字段版）
--       + 段2.5（gys 供应商补缺：M3 旧库 UI 配置丢失，随本基线补齐）
-- 幂等：ON CONFLICT (activation_code) DO UPDATE
-- ============================================================

-- 2. 激活映射：10 个新域（create + update 各一条；update 的 key_fields 留空——步骤①查重仅新建场景）
-- ─────────────────────────────────────────────

-- currency · 新建

INSERT INTO mdm_activation (id, activation_code, source_doc_type, cr_type, target_dict, target_table,
                                header_mapping, line_mappings, subject_name_field, key_fields, doc_code_rules,
                                header_groups, is_active)
VALUES ('mdm_act_bz_create', 'bz__create', 'bz', 'create', 'currency', 'cm_currency',
        '{"currency_code":"currency_code","name":"name","symbol":"symbol","decimal_places":"decimal_places","is_base":"is_base"}'::jsonb,
        '[]'::jsonb,
        'name',
        '[{"field":"currency_code","weight":60,"kind":"Exact","dedup":true},{"field":"name","weight":40,"kind":"EditDistance","dedup":true}]'::jsonb,
        '{}'::jsonb,
        '[{"groupCode":"base","groupName":"基本信息","fields":["currency_code","name","symbol"]},{"groupCode":"attr","groupName":"属性","fields":["decimal_places","is_base"]}]'::jsonb,
        TRUE)
ON CONFLICT (activation_code) DO UPDATE SET
    source_doc_type = EXCLUDED.source_doc_type, cr_type = EXCLUDED.cr_type,
    target_dict = EXCLUDED.target_dict, target_table = EXCLUDED.target_table,
    header_mapping = EXCLUDED.header_mapping, line_mappings = EXCLUDED.line_mappings,
    subject_name_field = EXCLUDED.subject_name_field, key_fields = EXCLUDED.key_fields,
    doc_code_rules = EXCLUDED.doc_code_rules, header_groups = EXCLUDED.header_groups,
    is_active = EXCLUDED.is_active, updated_at = now();

-- currency · 变更

INSERT INTO mdm_activation (id, activation_code, source_doc_type, cr_type, target_dict, target_table,
                                header_mapping, line_mappings, subject_name_field, key_fields, doc_code_rules,
                                header_groups, is_active)
VALUES ('mdm_act_bz_update', 'bz__update', 'bz', 'update', 'currency', 'cm_currency',
        '{"currency_code":"currency_code","name":"name","symbol":"symbol","decimal_places":"decimal_places","is_base":"is_base"}'::jsonb,
        '[]'::jsonb,
        'name',
        '[]'::jsonb,
        '{}'::jsonb,
        '[{"groupCode":"base","groupName":"基本信息","fields":["currency_code","name","symbol"]},{"groupCode":"attr","groupName":"属性","fields":["decimal_places","is_base"]}]'::jsonb,
        TRUE)
ON CONFLICT (activation_code) DO UPDATE SET
    source_doc_type = EXCLUDED.source_doc_type, cr_type = EXCLUDED.cr_type,
    target_dict = EXCLUDED.target_dict, target_table = EXCLUDED.target_table,
    header_mapping = EXCLUDED.header_mapping, line_mappings = EXCLUDED.line_mappings,
    subject_name_field = EXCLUDED.subject_name_field, key_fields = EXCLUDED.key_fields,
    doc_code_rules = EXCLUDED.doc_code_rules, header_groups = EXCLUDED.header_groups,
    is_active = EXCLUDED.is_active, updated_at = now();

-- uom · 新建

INSERT INTO mdm_activation (id, activation_code, source_doc_type, cr_type, target_dict, target_table,
                                header_mapping, line_mappings, subject_name_field, key_fields, doc_code_rules,
                                header_groups, is_active)
VALUES ('mdm_act_jldw_create', 'jldw__create', 'jldw', 'create', 'uom', 'cm_uom',
        '{"uom_code":"uom_code","name":"name","unit_type":"unit_type"}'::jsonb,
        '[]'::jsonb,
        'name',
        '[{"field":"uom_code","weight":60,"kind":"Exact","dedup":true},{"field":"name","weight":40,"kind":"EditDistance","dedup":true}]'::jsonb,
        '{}'::jsonb,
        '[{"groupCode":"base","groupName":"基本信息","fields":["uom_code","name","unit_type"]}]'::jsonb,
        TRUE)
ON CONFLICT (activation_code) DO UPDATE SET
    source_doc_type = EXCLUDED.source_doc_type, cr_type = EXCLUDED.cr_type,
    target_dict = EXCLUDED.target_dict, target_table = EXCLUDED.target_table,
    header_mapping = EXCLUDED.header_mapping, line_mappings = EXCLUDED.line_mappings,
    subject_name_field = EXCLUDED.subject_name_field, key_fields = EXCLUDED.key_fields,
    doc_code_rules = EXCLUDED.doc_code_rules, header_groups = EXCLUDED.header_groups,
    is_active = EXCLUDED.is_active, updated_at = now();

-- uom · 变更

INSERT INTO mdm_activation (id, activation_code, source_doc_type, cr_type, target_dict, target_table,
                                header_mapping, line_mappings, subject_name_field, key_fields, doc_code_rules,
                                header_groups, is_active)
VALUES ('mdm_act_jldw_update', 'jldw__update', 'jldw', 'update', 'uom', 'cm_uom',
        '{"uom_code":"uom_code","name":"name","unit_type":"unit_type"}'::jsonb,
        '[]'::jsonb,
        'name',
        '[]'::jsonb,
        '{}'::jsonb,
        '[{"groupCode":"base","groupName":"基本信息","fields":["uom_code","name","unit_type"]}]'::jsonb,
        TRUE)
ON CONFLICT (activation_code) DO UPDATE SET
    source_doc_type = EXCLUDED.source_doc_type, cr_type = EXCLUDED.cr_type,
    target_dict = EXCLUDED.target_dict, target_table = EXCLUDED.target_table,
    header_mapping = EXCLUDED.header_mapping, line_mappings = EXCLUDED.line_mappings,
    subject_name_field = EXCLUDED.subject_name_field, key_fields = EXCLUDED.key_fields,
    doc_code_rules = EXCLUDED.doc_code_rules, header_groups = EXCLUDED.header_groups,
    is_active = EXCLUDED.is_active, updated_at = now();

-- material_class · 新建

INSERT INTO mdm_activation (id, activation_code, source_doc_type, cr_type, target_dict, target_table,
                                header_mapping, line_mappings, subject_name_field, key_fields, doc_code_rules,
                                header_groups, is_active)
VALUES ('mdm_act_wldl_create', 'wldl__create', 'wldl', 'create', 'material_class', 'cm_material_class',
        '{"class_code":"class_code","name":"name","parent_id":"parent_id","class_type":"class_type"}'::jsonb,
        '[]'::jsonb,
        'name',
        '[{"field":"class_code","weight":50,"kind":"Exact","dedup":true},{"field":"name","weight":50,"kind":"EditDistance","dedup":true}]'::jsonb,
        '{}'::jsonb,
        '[{"groupCode":"base","groupName":"基本信息","fields":["class_code","name","parent_id"]},{"groupCode":"attr","groupName":"类别属性","fields":["class_type"]}]'::jsonb,
        TRUE)
ON CONFLICT (activation_code) DO UPDATE SET
    source_doc_type = EXCLUDED.source_doc_type, cr_type = EXCLUDED.cr_type,
    target_dict = EXCLUDED.target_dict, target_table = EXCLUDED.target_table,
    header_mapping = EXCLUDED.header_mapping, line_mappings = EXCLUDED.line_mappings,
    subject_name_field = EXCLUDED.subject_name_field, key_fields = EXCLUDED.key_fields,
    doc_code_rules = EXCLUDED.doc_code_rules, header_groups = EXCLUDED.header_groups,
    is_active = EXCLUDED.is_active, updated_at = now();

-- material_class · 变更

INSERT INTO mdm_activation (id, activation_code, source_doc_type, cr_type, target_dict, target_table,
                                header_mapping, line_mappings, subject_name_field, key_fields, doc_code_rules,
                                header_groups, is_active)
VALUES ('mdm_act_wldl_update', 'wldl__update', 'wldl', 'update', 'material_class', 'cm_material_class',
        '{"class_code":"class_code","name":"name","parent_id":"parent_id","class_type":"class_type"}'::jsonb,
        '[]'::jsonb,
        'name',
        '[]'::jsonb,
        '{}'::jsonb,
        '[{"groupCode":"base","groupName":"基本信息","fields":["class_code","name","parent_id"]},{"groupCode":"attr","groupName":"类别属性","fields":["class_type"]}]'::jsonb,
        TRUE)
ON CONFLICT (activation_code) DO UPDATE SET
    source_doc_type = EXCLUDED.source_doc_type, cr_type = EXCLUDED.cr_type,
    target_dict = EXCLUDED.target_dict, target_table = EXCLUDED.target_table,
    header_mapping = EXCLUDED.header_mapping, line_mappings = EXCLUDED.line_mappings,
    subject_name_field = EXCLUDED.subject_name_field, key_fields = EXCLUDED.key_fields,
    doc_code_rules = EXCLUDED.doc_code_rules, header_groups = EXCLUDED.header_groups,
    is_active = EXCLUDED.is_active, updated_at = now();

-- cost_center · 新建

INSERT INTO mdm_activation (id, activation_code, source_doc_type, cr_type, target_dict, target_table,
                                header_mapping, line_mappings, subject_name_field, key_fields, doc_code_rules,
                                header_groups, is_active)
VALUES ('mdm_act_cbzx_create', 'cbzx__create', 'cbzx', 'create', 'cost_center', 'cm_cost_center',
        '{"cost_center_code":"cost_center_code","name":"name","parent_id":"parent_id","dept_id":"dept_id"}'::jsonb,
        '[]'::jsonb,
        'name',
        '[{"field":"cost_center_code","weight":50,"kind":"Exact","dedup":true},{"field":"name","weight":50,"kind":"EditDistance","dedup":true}]'::jsonb,
        '{}'::jsonb,
        '[{"groupCode":"base","groupName":"基本信息","fields":["cost_center_code","name","parent_id"]},{"groupCode":"resp","groupName":"责任归属","fields":["dept_id"]}]'::jsonb,
        TRUE)
ON CONFLICT (activation_code) DO UPDATE SET
    source_doc_type = EXCLUDED.source_doc_type, cr_type = EXCLUDED.cr_type,
    target_dict = EXCLUDED.target_dict, target_table = EXCLUDED.target_table,
    header_mapping = EXCLUDED.header_mapping, line_mappings = EXCLUDED.line_mappings,
    subject_name_field = EXCLUDED.subject_name_field, key_fields = EXCLUDED.key_fields,
    doc_code_rules = EXCLUDED.doc_code_rules, header_groups = EXCLUDED.header_groups,
    is_active = EXCLUDED.is_active, updated_at = now();

-- cost_center · 变更

INSERT INTO mdm_activation (id, activation_code, source_doc_type, cr_type, target_dict, target_table,
                                header_mapping, line_mappings, subject_name_field, key_fields, doc_code_rules,
                                header_groups, is_active)
VALUES ('mdm_act_cbzx_update', 'cbzx__update', 'cbzx', 'update', 'cost_center', 'cm_cost_center',
        '{"cost_center_code":"cost_center_code","name":"name","parent_id":"parent_id","dept_id":"dept_id"}'::jsonb,
        '[]'::jsonb,
        'name',
        '[]'::jsonb,
        '{}'::jsonb,
        '[{"groupCode":"base","groupName":"基本信息","fields":["cost_center_code","name","parent_id"]},{"groupCode":"resp","groupName":"责任归属","fields":["dept_id"]}]'::jsonb,
        TRUE)
ON CONFLICT (activation_code) DO UPDATE SET
    source_doc_type = EXCLUDED.source_doc_type, cr_type = EXCLUDED.cr_type,
    target_dict = EXCLUDED.target_dict, target_table = EXCLUDED.target_table,
    header_mapping = EXCLUDED.header_mapping, line_mappings = EXCLUDED.line_mappings,
    subject_name_field = EXCLUDED.subject_name_field, key_fields = EXCLUDED.key_fields,
    doc_code_rules = EXCLUDED.doc_code_rules, header_groups = EXCLUDED.header_groups,
    is_active = EXCLUDED.is_active, updated_at = now();

-- profit_center · 新建

INSERT INTO mdm_activation (id, activation_code, source_doc_type, cr_type, target_dict, target_table,
                                header_mapping, line_mappings, subject_name_field, key_fields, doc_code_rules,
                                header_groups, is_active)
VALUES ('mdm_act_lrzx_create', 'lrzx__create', 'lrzx', 'create', 'profit_center', 'cm_profit_center',
        '{"profit_center_code":"profit_center_code","name":"name","parent_id":"parent_id"}'::jsonb,
        '[]'::jsonb,
        'name',
        '[{"field":"profit_center_code","weight":50,"kind":"Exact","dedup":true},{"field":"name","weight":50,"kind":"EditDistance","dedup":true}]'::jsonb,
        '{}'::jsonb,
        '[{"groupCode":"base","groupName":"基本信息","fields":["profit_center_code","name","parent_id"]}]'::jsonb,
        TRUE)
ON CONFLICT (activation_code) DO UPDATE SET
    source_doc_type = EXCLUDED.source_doc_type, cr_type = EXCLUDED.cr_type,
    target_dict = EXCLUDED.target_dict, target_table = EXCLUDED.target_table,
    header_mapping = EXCLUDED.header_mapping, line_mappings = EXCLUDED.line_mappings,
    subject_name_field = EXCLUDED.subject_name_field, key_fields = EXCLUDED.key_fields,
    doc_code_rules = EXCLUDED.doc_code_rules, header_groups = EXCLUDED.header_groups,
    is_active = EXCLUDED.is_active, updated_at = now();

-- profit_center · 变更

INSERT INTO mdm_activation (id, activation_code, source_doc_type, cr_type, target_dict, target_table,
                                header_mapping, line_mappings, subject_name_field, key_fields, doc_code_rules,
                                header_groups, is_active)
VALUES ('mdm_act_lrzx_update', 'lrzx__update', 'lrzx', 'update', 'profit_center', 'cm_profit_center',
        '{"profit_center_code":"profit_center_code","name":"name","parent_id":"parent_id"}'::jsonb,
        '[]'::jsonb,
        'name',
        '[]'::jsonb,
        '{}'::jsonb,
        '[{"groupCode":"base","groupName":"基本信息","fields":["profit_center_code","name","parent_id"]}]'::jsonb,
        TRUE)
ON CONFLICT (activation_code) DO UPDATE SET
    source_doc_type = EXCLUDED.source_doc_type, cr_type = EXCLUDED.cr_type,
    target_dict = EXCLUDED.target_dict, target_table = EXCLUDED.target_table,
    header_mapping = EXCLUDED.header_mapping, line_mappings = EXCLUDED.line_mappings,
    subject_name_field = EXCLUDED.subject_name_field, key_fields = EXCLUDED.key_fields,
    doc_code_rules = EXCLUDED.doc_code_rules, header_groups = EXCLUDED.header_groups,
    is_active = EXCLUDED.is_active, updated_at = now();

-- company · 新建

INSERT INTO mdm_activation (id, activation_code, source_doc_type, cr_type, target_dict, target_table,
                                header_mapping, line_mappings, subject_name_field, key_fields, doc_code_rules,
                                header_groups, is_active)
VALUES ('mdm_act_gs_create', 'gs__create', 'gs', 'create', 'company', 'cm_company',
        '{"company_code":"company_code","name":"name","short_name":"short_name","credit_code":"credit_code","legal_person":"legal_person","base_currency_id":"base_currency_id","registered_address":"registered_address"}'::jsonb,
        '[]'::jsonb,
        'name',
        '[{"field":"credit_code","weight":50,"kind":"Exact","dedup":true},{"field":"name","weight":50,"kind":"EditDistance","dedup":true}]'::jsonb,
        '{}'::jsonb,
        '[{"groupCode":"base","groupName":"基本信息","fields":["company_code","name","short_name"]},{"groupCode":"reg","groupName":"注册信息","fields":["credit_code","legal_person","registered_address"]},{"groupCode":"fin","groupName":"财务","fields":["base_currency_id"]}]'::jsonb,
        TRUE)
ON CONFLICT (activation_code) DO UPDATE SET
    source_doc_type = EXCLUDED.source_doc_type, cr_type = EXCLUDED.cr_type,
    target_dict = EXCLUDED.target_dict, target_table = EXCLUDED.target_table,
    header_mapping = EXCLUDED.header_mapping, line_mappings = EXCLUDED.line_mappings,
    subject_name_field = EXCLUDED.subject_name_field, key_fields = EXCLUDED.key_fields,
    doc_code_rules = EXCLUDED.doc_code_rules, header_groups = EXCLUDED.header_groups,
    is_active = EXCLUDED.is_active, updated_at = now();

-- company · 变更

INSERT INTO mdm_activation (id, activation_code, source_doc_type, cr_type, target_dict, target_table,
                                header_mapping, line_mappings, subject_name_field, key_fields, doc_code_rules,
                                header_groups, is_active)
VALUES ('mdm_act_gs_update', 'gs__update', 'gs', 'update', 'company', 'cm_company',
        '{"company_code":"company_code","name":"name","short_name":"short_name","credit_code":"credit_code","legal_person":"legal_person","base_currency_id":"base_currency_id","registered_address":"registered_address"}'::jsonb,
        '[]'::jsonb,
        'name',
        '[]'::jsonb,
        '{}'::jsonb,
        '[{"groupCode":"base","groupName":"基本信息","fields":["company_code","name","short_name"]},{"groupCode":"reg","groupName":"注册信息","fields":["credit_code","legal_person","registered_address"]},{"groupCode":"fin","groupName":"财务","fields":["base_currency_id"]}]'::jsonb,
        TRUE)
ON CONFLICT (activation_code) DO UPDATE SET
    source_doc_type = EXCLUDED.source_doc_type, cr_type = EXCLUDED.cr_type,
    target_dict = EXCLUDED.target_dict, target_table = EXCLUDED.target_table,
    header_mapping = EXCLUDED.header_mapping, line_mappings = EXCLUDED.line_mappings,
    subject_name_field = EXCLUDED.subject_name_field, key_fields = EXCLUDED.key_fields,
    doc_code_rules = EXCLUDED.doc_code_rules, header_groups = EXCLUDED.header_groups,
    is_active = EXCLUDED.is_active, updated_at = now();

-- organization · 新建

INSERT INTO mdm_activation (id, activation_code, source_doc_type, cr_type, target_dict, target_table,
                                header_mapping, line_mappings, subject_name_field, key_fields, doc_code_rules,
                                header_groups, is_active)
VALUES ('mdm_act_zz_create', 'zz__create', 'zz', 'create', 'organization', 'cm_organization',
        '{"org_code":"org_code","name":"name","parent_id":"parent_id","company_id":"company_id","org_type":"org_type"}'::jsonb,
        '[]'::jsonb,
        'name',
        '[{"field":"org_code","weight":50,"kind":"Exact","dedup":true},{"field":"name","weight":50,"kind":"EditDistance","dedup":true}]'::jsonb,
        '{}'::jsonb,
        '[{"groupCode":"base","groupName":"基本信息","fields":["org_code","name","parent_id"]},{"groupCode":"attr","groupName":"组织属性","fields":["company_id","org_type"]}]'::jsonb,
        TRUE)
ON CONFLICT (activation_code) DO UPDATE SET
    source_doc_type = EXCLUDED.source_doc_type, cr_type = EXCLUDED.cr_type,
    target_dict = EXCLUDED.target_dict, target_table = EXCLUDED.target_table,
    header_mapping = EXCLUDED.header_mapping, line_mappings = EXCLUDED.line_mappings,
    subject_name_field = EXCLUDED.subject_name_field, key_fields = EXCLUDED.key_fields,
    doc_code_rules = EXCLUDED.doc_code_rules, header_groups = EXCLUDED.header_groups,
    is_active = EXCLUDED.is_active, updated_at = now();

-- organization · 变更

INSERT INTO mdm_activation (id, activation_code, source_doc_type, cr_type, target_dict, target_table,
                                header_mapping, line_mappings, subject_name_field, key_fields, doc_code_rules,
                                header_groups, is_active)
VALUES ('mdm_act_zz_update', 'zz__update', 'zz', 'update', 'organization', 'cm_organization',
        '{"org_code":"org_code","name":"name","parent_id":"parent_id","company_id":"company_id","org_type":"org_type"}'::jsonb,
        '[]'::jsonb,
        'name',
        '[]'::jsonb,
        '{}'::jsonb,
        '[{"groupCode":"base","groupName":"基本信息","fields":["org_code","name","parent_id"]},{"groupCode":"attr","groupName":"组织属性","fields":["company_id","org_type"]}]'::jsonb,
        TRUE)
ON CONFLICT (activation_code) DO UPDATE SET
    source_doc_type = EXCLUDED.source_doc_type, cr_type = EXCLUDED.cr_type,
    target_dict = EXCLUDED.target_dict, target_table = EXCLUDED.target_table,
    header_mapping = EXCLUDED.header_mapping, line_mappings = EXCLUDED.line_mappings,
    subject_name_field = EXCLUDED.subject_name_field, key_fields = EXCLUDED.key_fields,
    doc_code_rules = EXCLUDED.doc_code_rules, header_groups = EXCLUDED.header_groups,
    is_active = EXCLUDED.is_active, updated_at = now();

-- department · 新建

INSERT INTO mdm_activation (id, activation_code, source_doc_type, cr_type, target_dict, target_table,
                                header_mapping, line_mappings, subject_name_field, key_fields, doc_code_rules,
                                header_groups, is_active)
VALUES ('mdm_act_bm_create', 'bm__create', 'bm', 'create', 'department', 'cm_department',
        '{"dept_code":"dept_code","name":"name","parent_id":"parent_id","org_id":"org_id"}'::jsonb,
        '[]'::jsonb,
        'name',
        '[{"field":"dept_code","weight":50,"kind":"Exact","dedup":true},{"field":"name","weight":50,"kind":"EditDistance","dedup":true}]'::jsonb,
        '{}'::jsonb,
        '[{"groupCode":"base","groupName":"基本信息","fields":["dept_code","name","parent_id"]},{"groupCode":"attr","groupName":"组织归属","fields":["org_id"]}]'::jsonb,
        TRUE)
ON CONFLICT (activation_code) DO UPDATE SET
    source_doc_type = EXCLUDED.source_doc_type, cr_type = EXCLUDED.cr_type,
    target_dict = EXCLUDED.target_dict, target_table = EXCLUDED.target_table,
    header_mapping = EXCLUDED.header_mapping, line_mappings = EXCLUDED.line_mappings,
    subject_name_field = EXCLUDED.subject_name_field, key_fields = EXCLUDED.key_fields,
    doc_code_rules = EXCLUDED.doc_code_rules, header_groups = EXCLUDED.header_groups,
    is_active = EXCLUDED.is_active, updated_at = now();

-- department · 变更

INSERT INTO mdm_activation (id, activation_code, source_doc_type, cr_type, target_dict, target_table,
                                header_mapping, line_mappings, subject_name_field, key_fields, doc_code_rules,
                                header_groups, is_active)
VALUES ('mdm_act_bm_update', 'bm__update', 'bm', 'update', 'department', 'cm_department',
        '{"dept_code":"dept_code","name":"name","parent_id":"parent_id","org_id":"org_id"}'::jsonb,
        '[]'::jsonb,
        'name',
        '[]'::jsonb,
        '{}'::jsonb,
        '[{"groupCode":"base","groupName":"基本信息","fields":["dept_code","name","parent_id"]},{"groupCode":"attr","groupName":"组织归属","fields":["org_id"]}]'::jsonb,
        TRUE)
ON CONFLICT (activation_code) DO UPDATE SET
    source_doc_type = EXCLUDED.source_doc_type, cr_type = EXCLUDED.cr_type,
    target_dict = EXCLUDED.target_dict, target_table = EXCLUDED.target_table,
    header_mapping = EXCLUDED.header_mapping, line_mappings = EXCLUDED.line_mappings,
    subject_name_field = EXCLUDED.subject_name_field, key_fields = EXCLUDED.key_fields,
    doc_code_rules = EXCLUDED.doc_code_rules, header_groups = EXCLUDED.header_groups,
    is_active = EXCLUDED.is_active, updated_at = now();

-- position · 新建

INSERT INTO mdm_activation (id, activation_code, source_doc_type, cr_type, target_dict, target_table,
                                header_mapping, line_mappings, subject_name_field, key_fields, doc_code_rules,
                                header_groups, is_active)
VALUES ('mdm_act_gw_create', 'gw__create', 'gw', 'create', 'position', 'cm_position',
        '{"position_code":"position_code","name":"name","job_family":"job_family","job_grade":"job_grade"}'::jsonb,
        '[]'::jsonb,
        'name',
        '[{"field":"position_code","weight":60,"kind":"Exact","dedup":true},{"field":"name","weight":40,"kind":"EditDistance","dedup":true}]'::jsonb,
        '{}'::jsonb,
        '[{"groupCode":"base","groupName":"基本信息","fields":["position_code","name"]},{"groupCode":"attr","groupName":"职级职族","fields":["job_family","job_grade"]}]'::jsonb,
        TRUE)
ON CONFLICT (activation_code) DO UPDATE SET
    source_doc_type = EXCLUDED.source_doc_type, cr_type = EXCLUDED.cr_type,
    target_dict = EXCLUDED.target_dict, target_table = EXCLUDED.target_table,
    header_mapping = EXCLUDED.header_mapping, line_mappings = EXCLUDED.line_mappings,
    subject_name_field = EXCLUDED.subject_name_field, key_fields = EXCLUDED.key_fields,
    doc_code_rules = EXCLUDED.doc_code_rules, header_groups = EXCLUDED.header_groups,
    is_active = EXCLUDED.is_active, updated_at = now();

-- position · 变更

INSERT INTO mdm_activation (id, activation_code, source_doc_type, cr_type, target_dict, target_table,
                                header_mapping, line_mappings, subject_name_field, key_fields, doc_code_rules,
                                header_groups, is_active)
VALUES ('mdm_act_gw_update', 'gw__update', 'gw', 'update', 'position', 'cm_position',
        '{"position_code":"position_code","name":"name","job_family":"job_family","job_grade":"job_grade"}'::jsonb,
        '[]'::jsonb,
        'name',
        '[]'::jsonb,
        '{}'::jsonb,
        '[{"groupCode":"base","groupName":"基本信息","fields":["position_code","name"]},{"groupCode":"attr","groupName":"职级职族","fields":["job_family","job_grade"]}]'::jsonb,
        TRUE)
ON CONFLICT (activation_code) DO UPDATE SET
    source_doc_type = EXCLUDED.source_doc_type, cr_type = EXCLUDED.cr_type,
    target_dict = EXCLUDED.target_dict, target_table = EXCLUDED.target_table,
    header_mapping = EXCLUDED.header_mapping, line_mappings = EXCLUDED.line_mappings,
    subject_name_field = EXCLUDED.subject_name_field, key_fields = EXCLUDED.key_fields,
    doc_code_rules = EXCLUDED.doc_code_rules, header_groups = EXCLUDED.header_groups,
    is_active = EXCLUDED.is_active, updated_at = now();

-- employee · 新建

INSERT INTO mdm_activation (id, activation_code, source_doc_type, cr_type, target_dict, target_table,
                                header_mapping, line_mappings, subject_name_field, key_fields, doc_code_rules,
                                header_groups, is_active)
VALUES ('mdm_act_yg_create', 'yg__create', 'yg', 'create', 'employee', 'cm_employee',
        '{"emp_no":"emp_no","name":"name","company_id":"company_id","dept_id":"dept_id","position_id":"position_id","mobile":"mobile","email":"email","hire_date":"hire_date","emp_status":"emp_status"}'::jsonb,
        '[]'::jsonb,
        'name',
        '[{"field":"emp_no","weight":50,"kind":"Exact","dedup":true},{"field":"name","weight":30,"kind":"EditDistance","dedup":true},{"field":"mobile","weight":20,"kind":"Exact","dedup":true}]'::jsonb,
        '{}'::jsonb,
        '[{"groupCode":"base","groupName":"基本信息","fields":["emp_no","name","mobile","email"]},{"groupCode":"org","groupName":"组织归属","fields":["company_id","dept_id","position_id"]},{"groupCode":"attr","groupName":"人事属性","fields":["hire_date","emp_status"]}]'::jsonb,
        TRUE)
ON CONFLICT (activation_code) DO UPDATE SET
    source_doc_type = EXCLUDED.source_doc_type, cr_type = EXCLUDED.cr_type,
    target_dict = EXCLUDED.target_dict, target_table = EXCLUDED.target_table,
    header_mapping = EXCLUDED.header_mapping, line_mappings = EXCLUDED.line_mappings,
    subject_name_field = EXCLUDED.subject_name_field, key_fields = EXCLUDED.key_fields,
    doc_code_rules = EXCLUDED.doc_code_rules, header_groups = EXCLUDED.header_groups,
    is_active = EXCLUDED.is_active, updated_at = now();

-- employee · 变更

INSERT INTO mdm_activation (id, activation_code, source_doc_type, cr_type, target_dict, target_table,
                                header_mapping, line_mappings, subject_name_field, key_fields, doc_code_rules,
                                header_groups, is_active)
VALUES ('mdm_act_yg_update', 'yg__update', 'yg', 'update', 'employee', 'cm_employee',
        '{"emp_no":"emp_no","name":"name","company_id":"company_id","dept_id":"dept_id","position_id":"position_id","mobile":"mobile","email":"email","hire_date":"hire_date","emp_status":"emp_status"}'::jsonb,
        '[]'::jsonb,
        'name',
        '[]'::jsonb,
        '{}'::jsonb,
        '[{"groupCode":"base","groupName":"基本信息","fields":["emp_no","name","mobile","email"]},{"groupCode":"org","groupName":"组织归属","fields":["company_id","dept_id","position_id"]},{"groupCode":"attr","groupName":"人事属性","fields":["hire_date","emp_status"]}]'::jsonb,
        TRUE)
ON CONFLICT (activation_code) DO UPDATE SET
    source_doc_type = EXCLUDED.source_doc_type, cr_type = EXCLUDED.cr_type,
    target_dict = EXCLUDED.target_dict, target_table = EXCLUDED.target_table,
    header_mapping = EXCLUDED.header_mapping, line_mappings = EXCLUDED.line_mappings,
    subject_name_field = EXCLUDED.subject_name_field, key_fields = EXCLUDED.key_fields,
    doc_code_rules = EXCLUDED.doc_code_rules, header_groups = EXCLUDED.header_groups,
    is_active = EXCLUDED.is_active, updated_at = now();


-- ─────────────────────────────────────────────

-- 2.5 供应商（gys）激活映射补缺：M3 时代靠 UI 配置存于旧运行库，库重建后缺失
--     （MDM_GYS 编码规则兜底同因）。字段对齐现行 supplier 字典：
--     头 name/short_name/tax_no/credit_code/phone + supplier_bank（cm_bank_account）明细。
-- ─────────────────────────────────────────────

-- supplier · 新建

INSERT INTO mdm_activation (id, activation_code, source_doc_type, cr_type, target_dict, target_table,
                                header_mapping, line_mappings, subject_name_field, key_fields, doc_code_rules,
                                header_groups, is_active)
VALUES ('mdm_act_gys_create', 'gys__create', 'gys', 'create', 'supplier', 'cm_supplier',
        '{"name":"name","short_name":"short_name","tax_no":"tax_no","credit_code":"credit_code","phone":"phone"}'::jsonb,
        '[{"lineType":"bank","targetDict":"supplier_bank","targetTable":"cm_bank_account","parentIdField":"supplier_id","fields":{"account_no":"account_no","bank_name":"bank_name"},"fieldOrder":["account_no","bank_name"]}]'::jsonb,
        'name',
        '[{"field":"credit_code","weight":40,"kind":"Exact","dedup":true},{"field":"tax_no","weight":20,"kind":"Exact","dedup":true},{"field":"name","weight":40,"kind":"EditDistance","dedup":true}]'::jsonb,
        '{}'::jsonb,
        '[{"groupCode":"base","groupName":"基本信息","fields":["name","short_name"]},{"groupCode":"qual","groupName":"资质与联系","fields":["tax_no","credit_code","phone"]}]'::jsonb,
        TRUE)
ON CONFLICT (activation_code) DO UPDATE SET
    source_doc_type = EXCLUDED.source_doc_type, cr_type = EXCLUDED.cr_type,
    target_dict = EXCLUDED.target_dict, target_table = EXCLUDED.target_table,
    header_mapping = EXCLUDED.header_mapping, line_mappings = EXCLUDED.line_mappings,
    subject_name_field = EXCLUDED.subject_name_field, key_fields = EXCLUDED.key_fields,
    doc_code_rules = EXCLUDED.doc_code_rules, header_groups = EXCLUDED.header_groups,
    is_active = EXCLUDED.is_active, updated_at = now();

-- supplier · 变更

INSERT INTO mdm_activation (id, activation_code, source_doc_type, cr_type, target_dict, target_table,
                                header_mapping, line_mappings, subject_name_field, key_fields, doc_code_rules,
                                header_groups, is_active)
VALUES ('mdm_act_gys_update', 'gys__update', 'gys', 'update', 'supplier', 'cm_supplier',
        '{"name":"name","short_name":"short_name","tax_no":"tax_no","credit_code":"credit_code","phone":"phone"}'::jsonb,
        '[{"lineType":"bank","targetDict":"supplier_bank","targetTable":"cm_bank_account","parentIdField":"supplier_id","fields":{"account_no":"account_no","bank_name":"bank_name"},"fieldOrder":["account_no","bank_name"]}]'::jsonb,
        'name',
        '[]'::jsonb,
        '{}'::jsonb,
        '[{"groupCode":"base","groupName":"基本信息","fields":["name","short_name"]},{"groupCode":"qual","groupName":"资质与联系","fields":["tax_no","credit_code","phone"]}]'::jsonb,
        TRUE)
ON CONFLICT (activation_code) DO UPDATE SET
    source_doc_type = EXCLUDED.source_doc_type, cr_type = EXCLUDED.cr_type,
    target_dict = EXCLUDED.target_dict, target_table = EXCLUDED.target_table,
    header_mapping = EXCLUDED.header_mapping, line_mappings = EXCLUDED.line_mappings,
    subject_name_field = EXCLUDED.subject_name_field, key_fields = EXCLUDED.key_fields,
    doc_code_rules = EXCLUDED.doc_code_rules, header_groups = EXCLUDED.header_groups,
    is_active = EXCLUDED.is_active, updated_at = now();


-- 3. 第一批三域激活映射（kh/wl/kj，深化字段版：客户商务/客户经理/地址明细、物料分类/多单位、科目辅助核算）
-- ─────────────────────────────────────────────

-- customer · 新建

INSERT INTO mdm_activation (id, activation_code, source_doc_type, cr_type, target_dict, target_table,
                                header_mapping, line_mappings, subject_name_field, key_fields, doc_code_rules,
                                header_groups, is_active)
VALUES ('mdm_act_kh_create', 'kh__create', 'kh', 'create', 'customer', 'cm_customer',
        '{"name":"name","short_name":"short_name","customer_type":"customer_type","credit_level":"credit_level","credit_code":"credit_code","tax_no":"tax_no","phone":"phone","address":"address","credit_limit":"credit_limit","payment_term":"payment_term","invoice_type":"invoice_type","industry":"industry","customer_manager_id":"customer_manager_id","settle_currency_id":"settle_currency_id"}'::jsonb,
        '[{"lineType":"bank","targetDict":"customer_bank","targetTable":"cm_customer_bank","parentIdField":"customer_id","fields":{"account_no":"account_no","account_name":"account_name","bank_name":"bank_name","is_default":"is_default"},"fieldOrder":["account_no","account_name","bank_name","is_default"]},{"lineType":"contact","targetDict":"customer_contact","targetTable":"cm_customer_contact","parentIdField":"customer_id","fields":{"contact_name":"contact_name","position":"position","phone":"phone","email":"email"},"fieldOrder":["contact_name","position","phone","email"]},{"lineType":"address","targetDict":"customer_address","targetTable":"cm_customer_address","parentIdField":"customer_id","fields":{"address_type":"address_type","province":"province","city":"city","district":"district","address_detail":"address_detail","receiver":"receiver","receiver_phone":"receiver_phone","is_default":"is_default"},"fieldOrder":["address_type","province","city","district","address_detail","receiver","receiver_phone","is_default"]}]'::jsonb,
        'name',
        '[{"field":"credit_code","weight":40,"kind":"Exact","dedup":true},{"field":"name","weight":60,"kind":"EditDistance","dedup":true}]'::jsonb,
        '{}'::jsonb,
        '[{"groupCode":"base","groupName":"基本信息","fields":["name","short_name","customer_type","credit_level"]},{"groupCode":"qual","groupName":"资质与联系","fields":["credit_code","tax_no","phone","address"]},{"groupCode":"biz","groupName":"商务信息","fields":["credit_limit","payment_term","invoice_type","industry"]},{"groupCode":"mgr","groupName":"归属","fields":["customer_manager_id","settle_currency_id"]}]'::jsonb,
        TRUE)
ON CONFLICT (activation_code) DO UPDATE SET
    source_doc_type = EXCLUDED.source_doc_type, cr_type = EXCLUDED.cr_type,
    target_dict = EXCLUDED.target_dict, target_table = EXCLUDED.target_table,
    header_mapping = EXCLUDED.header_mapping, line_mappings = EXCLUDED.line_mappings,
    subject_name_field = EXCLUDED.subject_name_field, key_fields = EXCLUDED.key_fields,
    doc_code_rules = EXCLUDED.doc_code_rules, header_groups = EXCLUDED.header_groups,
    is_active = EXCLUDED.is_active, updated_at = now();

-- customer · 变更

INSERT INTO mdm_activation (id, activation_code, source_doc_type, cr_type, target_dict, target_table,
                                header_mapping, line_mappings, subject_name_field, key_fields, doc_code_rules,
                                header_groups, is_active)
VALUES ('mdm_act_kh_update', 'kh__update', 'kh', 'update', 'customer', 'cm_customer',
        '{"name":"name","short_name":"short_name","customer_type":"customer_type","credit_level":"credit_level","credit_code":"credit_code","tax_no":"tax_no","phone":"phone","address":"address","credit_limit":"credit_limit","payment_term":"payment_term","invoice_type":"invoice_type","industry":"industry","customer_manager_id":"customer_manager_id","settle_currency_id":"settle_currency_id"}'::jsonb,
        '[{"lineType":"bank","targetDict":"customer_bank","targetTable":"cm_customer_bank","parentIdField":"customer_id","fields":{"account_no":"account_no","account_name":"account_name","bank_name":"bank_name","is_default":"is_default"},"fieldOrder":["account_no","account_name","bank_name","is_default"]},{"lineType":"contact","targetDict":"customer_contact","targetTable":"cm_customer_contact","parentIdField":"customer_id","fields":{"contact_name":"contact_name","position":"position","phone":"phone","email":"email"},"fieldOrder":["contact_name","position","phone","email"]},{"lineType":"address","targetDict":"customer_address","targetTable":"cm_customer_address","parentIdField":"customer_id","fields":{"address_type":"address_type","province":"province","city":"city","district":"district","address_detail":"address_detail","receiver":"receiver","receiver_phone":"receiver_phone","is_default":"is_default"},"fieldOrder":["address_type","province","city","district","address_detail","receiver","receiver_phone","is_default"]}]'::jsonb,
        'name',
        '[]'::jsonb,
        '{}'::jsonb,
        '[{"groupCode":"base","groupName":"基本信息","fields":["name","short_name","customer_type","credit_level"]},{"groupCode":"qual","groupName":"资质与联系","fields":["credit_code","tax_no","phone","address"]},{"groupCode":"biz","groupName":"商务信息","fields":["credit_limit","payment_term","invoice_type","industry"]},{"groupCode":"mgr","groupName":"归属","fields":["customer_manager_id","settle_currency_id"]}]'::jsonb,
        TRUE)
ON CONFLICT (activation_code) DO UPDATE SET
    source_doc_type = EXCLUDED.source_doc_type, cr_type = EXCLUDED.cr_type,
    target_dict = EXCLUDED.target_dict, target_table = EXCLUDED.target_table,
    header_mapping = EXCLUDED.header_mapping, line_mappings = EXCLUDED.line_mappings,
    subject_name_field = EXCLUDED.subject_name_field, key_fields = EXCLUDED.key_fields,
    doc_code_rules = EXCLUDED.doc_code_rules, header_groups = EXCLUDED.header_groups,
    is_active = EXCLUDED.is_active, updated_at = now();

-- material · 新建

INSERT INTO mdm_activation (id, activation_code, source_doc_type, cr_type, target_dict, target_table,
                                header_mapping, line_mappings, subject_name_field, key_fields, doc_code_rules,
                                header_groups, is_active)
VALUES ('mdm_act_wl_create', 'wl__create', 'wl', 'create', 'material', 'cm_material',
        '{"name":"name","short_name":"short_name","spec":"spec","model":"model","class_id":"class_id","material_type":"material_type","barcode":"barcode","base_uom_id":"base_uom_id","purchase_uom_id":"purchase_uom_id","stock_uom_id":"stock_uom_id","purchase_rate":"purchase_rate","brand":"brand","origin":"origin","net_weight":"net_weight","shelf_life_days":"shelf_life_days","batch_flag":"batch_flag","serial_flag":"serial_flag","hs_code":"hs_code","long_desc":"long_desc"}'::jsonb,
        '[]'::jsonb,
        'name',
        '[{"field":"name","weight":50,"kind":"EditDistance","dedup":true},{"field":"spec","weight":25,"kind":"Exact","dedup":true},{"field":"model","weight":25,"kind":"Exact","dedup":true}]'::jsonb,
        '{}'::jsonb,
        '[{"groupCode":"base","groupName":"基本信息","fields":["name","short_name","material_type","class_id"]},{"groupCode":"spec","groupName":"规格属性","fields":["spec","model","barcode","brand","origin"]},{"groupCode":"uom","groupName":"单位体系","fields":["base_uom_id","purchase_uom_id","stock_uom_id","purchase_rate"]},{"groupCode":"ext","groupName":"扩展属性","fields":["net_weight","shelf_life_days","batch_flag","serial_flag","hs_code","long_desc"]}]'::jsonb,
        TRUE)
ON CONFLICT (activation_code) DO UPDATE SET
    source_doc_type = EXCLUDED.source_doc_type, cr_type = EXCLUDED.cr_type,
    target_dict = EXCLUDED.target_dict, target_table = EXCLUDED.target_table,
    header_mapping = EXCLUDED.header_mapping, line_mappings = EXCLUDED.line_mappings,
    subject_name_field = EXCLUDED.subject_name_field, key_fields = EXCLUDED.key_fields,
    doc_code_rules = EXCLUDED.doc_code_rules, header_groups = EXCLUDED.header_groups,
    is_active = EXCLUDED.is_active, updated_at = now();

-- material · 变更

INSERT INTO mdm_activation (id, activation_code, source_doc_type, cr_type, target_dict, target_table,
                                header_mapping, line_mappings, subject_name_field, key_fields, doc_code_rules,
                                header_groups, is_active)
VALUES ('mdm_act_wl_update', 'wl__update', 'wl', 'update', 'material', 'cm_material',
        '{"name":"name","short_name":"short_name","spec":"spec","model":"model","class_id":"class_id","material_type":"material_type","barcode":"barcode","base_uom_id":"base_uom_id","purchase_uom_id":"purchase_uom_id","stock_uom_id":"stock_uom_id","purchase_rate":"purchase_rate","brand":"brand","origin":"origin","net_weight":"net_weight","shelf_life_days":"shelf_life_days","batch_flag":"batch_flag","serial_flag":"serial_flag","hs_code":"hs_code","long_desc":"long_desc"}'::jsonb,
        '[]'::jsonb,
        'name',
        '[]'::jsonb,
        '{}'::jsonb,
        '[{"groupCode":"base","groupName":"基本信息","fields":["name","short_name","material_type","class_id"]},{"groupCode":"spec","groupName":"规格属性","fields":["spec","model","barcode","brand","origin"]},{"groupCode":"uom","groupName":"单位体系","fields":["base_uom_id","purchase_uom_id","stock_uom_id","purchase_rate"]},{"groupCode":"ext","groupName":"扩展属性","fields":["net_weight","shelf_life_days","batch_flag","serial_flag","hs_code","long_desc"]}]'::jsonb,
        TRUE)
ON CONFLICT (activation_code) DO UPDATE SET
    source_doc_type = EXCLUDED.source_doc_type, cr_type = EXCLUDED.cr_type,
    target_dict = EXCLUDED.target_dict, target_table = EXCLUDED.target_table,
    header_mapping = EXCLUDED.header_mapping, line_mappings = EXCLUDED.line_mappings,
    subject_name_field = EXCLUDED.subject_name_field, key_fields = EXCLUDED.key_fields,
    doc_code_rules = EXCLUDED.doc_code_rules, header_groups = EXCLUDED.header_groups,
    is_active = EXCLUDED.is_active, updated_at = now();

-- gl_account · 新建

INSERT INTO mdm_activation (id, activation_code, source_doc_type, cr_type, target_dict, target_table,
                                header_mapping, line_mappings, subject_name_field, key_fields, doc_code_rules,
                                header_groups, is_active)
VALUES ('mdm_act_kj_create', 'kj__create', 'kj', 'create', 'gl_account', 'cm_gl_account',
        '{"acct_no":"acct_no","name":"name","parent_id":"parent_id","acct_type":"acct_type","direction":"direction","aux_biz_partner":"aux_biz_partner","aux_department":"aux_department","aux_employee":"aux_employee","aux_project":"aux_project","is_cash_flow":"is_cash_flow","foreign_currency_flag":"foreign_currency_flag","quantity_flag":"quantity_flag","ledger_format":"ledger_format"}'::jsonb,
        '[]'::jsonb,
        'name',
        '[{"field":"acct_no","weight":60,"kind":"Exact","dedup":true},{"field":"name","weight":40,"kind":"EditDistance","dedup":true}]'::jsonb,
        '{}'::jsonb,
        '[{"groupCode":"base","groupName":"基本信息","fields":["acct_no","name","parent_id"]},{"groupCode":"attr","groupName":"科目属性","fields":["acct_type","direction"]},{"groupCode":"aux","groupName":"辅助核算","fields":["aux_biz_partner","aux_department","aux_employee","aux_project"]},{"groupCode":"gl","groupName":"核算控制","fields":["is_cash_flow","foreign_currency_flag","quantity_flag","ledger_format"]}]'::jsonb,
        TRUE)
ON CONFLICT (activation_code) DO UPDATE SET
    source_doc_type = EXCLUDED.source_doc_type, cr_type = EXCLUDED.cr_type,
    target_dict = EXCLUDED.target_dict, target_table = EXCLUDED.target_table,
    header_mapping = EXCLUDED.header_mapping, line_mappings = EXCLUDED.line_mappings,
    subject_name_field = EXCLUDED.subject_name_field, key_fields = EXCLUDED.key_fields,
    doc_code_rules = EXCLUDED.doc_code_rules, header_groups = EXCLUDED.header_groups,
    is_active = EXCLUDED.is_active, updated_at = now();

-- gl_account · 变更

INSERT INTO mdm_activation (id, activation_code, source_doc_type, cr_type, target_dict, target_table,
                                header_mapping, line_mappings, subject_name_field, key_fields, doc_code_rules,
                                header_groups, is_active)
VALUES ('mdm_act_kj_update', 'kj__update', 'kj', 'update', 'gl_account', 'cm_gl_account',
        '{"acct_no":"acct_no","name":"name","parent_id":"parent_id","acct_type":"acct_type","direction":"direction","aux_biz_partner":"aux_biz_partner","aux_department":"aux_department","aux_employee":"aux_employee","aux_project":"aux_project","is_cash_flow":"is_cash_flow","foreign_currency_flag":"foreign_currency_flag","quantity_flag":"quantity_flag","ledger_format":"ledger_format"}'::jsonb,
        '[]'::jsonb,
        'name',
        '[]'::jsonb,
        '{}'::jsonb,
        '[{"groupCode":"base","groupName":"基本信息","fields":["acct_no","name","parent_id"]},{"groupCode":"attr","groupName":"科目属性","fields":["acct_type","direction"]},{"groupCode":"aux","groupName":"辅助核算","fields":["aux_biz_partner","aux_department","aux_employee","aux_project"]},{"groupCode":"gl","groupName":"核算控制","fields":["is_cash_flow","foreign_currency_flag","quantity_flag","ledger_format"]}]'::jsonb,
        TRUE)
ON CONFLICT (activation_code) DO UPDATE SET
    source_doc_type = EXCLUDED.source_doc_type, cr_type = EXCLUDED.cr_type,
    target_dict = EXCLUDED.target_dict, target_table = EXCLUDED.target_table,
    header_mapping = EXCLUDED.header_mapping, line_mappings = EXCLUDED.line_mappings,
    subject_name_field = EXCLUDED.subject_name_field, key_fields = EXCLUDED.key_fields,
    doc_code_rules = EXCLUDED.doc_code_rules, header_groups = EXCLUDED.header_groups,
    is_active = EXCLUDED.is_active, updated_at = now();





-- ─────────────────────────────────────────────


-- ============================================================
-- 2. 编码规则（cmx_code_rule）
-- 来源：迁移 20260818_001 段1（MDM 多域 14 条）
--       + 迁移 20260813_002（MDM_BILL 单据号保底，排段尾防与 14 条顺排 id 混淆）
-- 幂等：ON CONFLICT (rule_code) WHERE archived = 0 DO NOTHING
-- ============================================================

-- 1. 编码规则 cmx_code_rule（id 9000000000000002~0015 顺排，MDM_BILL=…0001 已占）
--    字典 code 铸号：激活器读 dictMeta.codeRule.ruleCode。漏配不报错，code 退化为占位码——故必须 seed。
-- ─────────────────────────────────────────────


INSERT INTO cmx_code_rule (id, rule_code, rule_name, mode, segments, joiner, is_active)
VALUES (9000000000000002, 'MDM_KH', '客户主数据编码（CUS+日期+流水）', 'auto',
        '[{"type":"const","value":"CUS"},{"type":"dateSerial","format":"YYYYMMDD","width":4,"start":1}]'::jsonb,
        '', TRUE)
ON CONFLICT (rule_code) WHERE archived = 0 DO NOTHING;

-- 物料主数据编码：MAT + YYYYMMDD + 4位日流水 → MAT202608180001
INSERT INTO cmx_code_rule (id, rule_code, rule_name, mode, segments, joiner, is_active)
VALUES (9000000000000003, 'MDM_WL', '物料主数据编码（MAT+日期+流水）', 'auto',
        '[{"type":"const","value":"MAT"},{"type":"dateSerial","format":"YYYYMMDD","width":4,"start":1}]'::jsonb,
        '', TRUE)
ON CONFLICT (rule_code) WHERE archived = 0 DO NOTHING;

-- 会计科目编码：ref 段取行字段 acct_no（用户在 CR 填的科目号，如 1001 / 100101）→ code = 科目号。
-- 说明：激活器 create 分支会先用占位码覆盖 header_row.code，仅当 dictMeta.codeRule 铸号成功才能再覆盖，
--       故科目号走 ref 段「借铸号通道」写入 code 列——code 与 acct_no 恒等，无需改 Rust 代码。
--       acct_no 为空时铸出空串（NOT NULL 允许），科目号在 CR 表单为必填，正常不会发生。
INSERT INTO cmx_code_rule (id, rule_code, rule_name, mode, segments, joiner, is_active)
VALUES (9000000000000004, 'MDM_KJ', '会计科目编码（取科目号 acct_no 原值）', 'auto',
        '[{"type":"ref","field":"acct_no"}]'::jsonb,
        '', TRUE)
ON CONFLICT (rule_code) WHERE archived = 0 DO NOTHING;




INSERT INTO cmx_code_rule (id, rule_code, rule_name, mode, segments, joiner, is_active)
VALUES (9000000000000005, 'MDM_BZ', '币种编码（取 ISO 币种码）', 'auto',
        '[{"type":"ref","field":"currency_code"}]'::jsonb,
        '', TRUE)
ON CONFLICT (rule_code) WHERE archived = 0 DO NOTHING;

INSERT INTO cmx_code_rule (id, rule_code, rule_name, mode, segments, joiner, is_active)
VALUES (9000000000000006, 'MDM_JLDW', '计量单位编码（取单位编码）', 'auto',
        '[{"type":"ref","field":"uom_code"}]'::jsonb,
        '', TRUE)
ON CONFLICT (rule_code) WHERE archived = 0 DO NOTHING;

INSERT INTO cmx_code_rule (id, rule_code, rule_name, mode, segments, joiner, is_active)
VALUES (9000000000000007, 'MDM_WLDL', '物料分类编码（取分类编码）', 'auto',
        '[{"type":"ref","field":"class_code"}]'::jsonb,
        '', TRUE)
ON CONFLICT (rule_code) WHERE archived = 0 DO NOTHING;

INSERT INTO cmx_code_rule (id, rule_code, rule_name, mode, segments, joiner, is_active)
VALUES (9000000000000008, 'MDM_CBZX', '成本中心编码（取中心编码）', 'auto',
        '[{"type":"ref","field":"cost_center_code"}]'::jsonb,
        '', TRUE)
ON CONFLICT (rule_code) WHERE archived = 0 DO NOTHING;

INSERT INTO cmx_code_rule (id, rule_code, rule_name, mode, segments, joiner, is_active)
VALUES (9000000000000009, 'MDM_LRZX', '利润中心编码（取中心编码）', 'auto',
        '[{"type":"ref","field":"profit_center_code"}]'::jsonb,
        '', TRUE)
ON CONFLICT (rule_code) WHERE archived = 0 DO NOTHING;

INSERT INTO cmx_code_rule (id, rule_code, rule_name, mode, segments, joiner, is_active)
VALUES (9000000000000010, 'MDM_GS', '公司编码（取公司编码）', 'auto',
        '[{"type":"ref","field":"company_code"}]'::jsonb,
        '', TRUE)
ON CONFLICT (rule_code) WHERE archived = 0 DO NOTHING;

INSERT INTO cmx_code_rule (id, rule_code, rule_name, mode, segments, joiner, is_active)
VALUES (9000000000000011, 'MDM_ZZ', '组织编码（取组织编码）', 'auto',
        '[{"type":"ref","field":"org_code"}]'::jsonb,
        '', TRUE)
ON CONFLICT (rule_code) WHERE archived = 0 DO NOTHING;

INSERT INTO cmx_code_rule (id, rule_code, rule_name, mode, segments, joiner, is_active)
VALUES (9000000000000012, 'MDM_BM', '部门编码（取部门编码）', 'auto',
        '[{"type":"ref","field":"dept_code"}]'::jsonb,
        '', TRUE)
ON CONFLICT (rule_code) WHERE archived = 0 DO NOTHING;

INSERT INTO cmx_code_rule (id, rule_code, rule_name, mode, segments, joiner, is_active)
VALUES (9000000000000013, 'MDM_GW', '岗位编码（取岗位编码）', 'auto',
        '[{"type":"ref","field":"position_code"}]'::jsonb,
        '', TRUE)
ON CONFLICT (rule_code) WHERE archived = 0 DO NOTHING;

INSERT INTO cmx_code_rule (id, rule_code, rule_name, mode, segments, joiner, is_active)
VALUES (9000000000000014, 'MDM_YG', '员工编码（取工号）', 'auto',
        '[{"type":"ref","field":"emp_no"}]'::jsonb,
        '', TRUE)
ON CONFLICT (rule_code) WHERE archived = 0 DO NOTHING;




INSERT INTO cmx_code_rule (id, rule_code, rule_name, mode, segments, joiner, is_active)
VALUES (9000000000000015, 'MDM_GYS', '供应商主数据编码（SUP+日期+流水）', 'auto',
        '[{"type":"const","value":"SUP"},{"type":"dateSerial","format":"YYYYMMDD","width":4,"start":1}]'::jsonb,
        '', TRUE)
ON CONFLICT (rule_code) WHERE archived = 0 DO NOTHING;

-- ─────────────────────────────────────────────


-- MDM 变更申请单据号保底规则（20260813_002）
INSERT INTO cmx_code_rule (id, rule_code, rule_name, mode, segments, joiner, is_active)
VALUES (9000000000000001, 'MDM_BILL', 'MDM 变更申请单据号（CR+日期+流水）', 'auto',
        '[{"type":"const","value":"CR"},{"type":"dateSerial","format":"YYYYMMDD","width":6,"start":1}]'::jsonb,
        '', TRUE)
ON CONFLICT (rule_code) WHERE archived = 0 DO NOTHING;

-- ============================================================
-- 3. 查重规则（md_match_config）
-- ============================================================

-- Seed: supplier 默认查重规则（id 固定值 1，应用层 next_pk_id 不会冲突）
INSERT INTO md_match_config (id, rule_name, dict_code, target_table, specs, cluster_keys, survive_fields, thresholds, is_active)
SELECT 1, '供应商默认查重', 'supplier', 'cm_supplier',
       '[{"field":"credit_code","weight":40,"kind":"Exact"},{"field":"tax_no","weight":30,"kind":"Exact"},{"field":"name","weight":30,"kind":"EditDistance"}]'::jsonb,
       '["credit_code","tax_no","name"]'::jsonb,
       '["name","tax_no","credit_code","short_name","phone"]'::jsonb,
       '{"auto_merge":95,"review":80}'::jsonb,
       TRUE
WHERE NOT EXISTS (SELECT 1 FROM md_match_config WHERE dict_code = 'supplier' AND rule_name = '供应商默认查重');

-- ---- MDM 多域查重规则 13 条（20260818_001 段4） ----
-- 4. 查重规则 md_match_config（id 2~14 顺排；NOT EXISTS 防重）
-- ─────────────────────────────────────────────


INSERT INTO md_match_config (id, rule_name, dict_code, target_table, specs, cluster_keys, survive_fields, thresholds, is_active)
SELECT 2, '客户默认查重', 'customer', 'cm_customer',
       '[{"field":"credit_code","weight":40,"kind":"Exact"},{"field":"name","weight":60,"kind":"EditDistance"}]'::jsonb,
       '["credit_code","name"]'::jsonb,
       '["name","short_name","customer_type","credit_level","credit_code","tax_no","phone","address"]'::jsonb,
       '{"auto_merge":95,"review":80}'::jsonb,
       TRUE
WHERE NOT EXISTS (SELECT 1 FROM md_match_config WHERE dict_code = 'customer' AND rule_name = '客户默认查重');

-- 物料默认查重：名称编辑距离 + 规格/型号精确（同名不同规格是不同物料）
INSERT INTO md_match_config (id, rule_name, dict_code, target_table, specs, cluster_keys, survive_fields, thresholds, is_active)
SELECT 3, '物料默认查重', 'material', 'cm_material',
       '[{"field":"name","weight":50,"kind":"EditDistance"},{"field":"spec","weight":25,"kind":"Exact"},{"field":"model","weight":25,"kind":"Exact"}]'::jsonb,
       '["name","spec","model"]'::jsonb,
       '["name","short_name","spec","model","unit","material_type","barcode"]'::jsonb,
       '{"auto_merge":95,"review":80}'::jsonb,
       TRUE
WHERE NOT EXISTS (SELECT 1 FROM md_match_config WHERE dict_code = 'material' AND rule_name = '物料默认查重');

-- 科目默认查重：科目编码精确 + 名称编辑距离（survive 不含 parent_id/full_path 等树形列——层级结构不参与字段存活裁决）
INSERT INTO md_match_config (id, rule_name, dict_code, target_table, specs, cluster_keys, survive_fields, thresholds, is_active)
SELECT 4, '科目默认查重', 'gl_account', 'cm_gl_account',
       '[{"field":"acct_no","weight":60,"kind":"Exact"},{"field":"name","weight":40,"kind":"EditDistance"}]'::jsonb,
       '["acct_no","name"]'::jsonb,
       '["name","acct_no","acct_type","direction"]'::jsonb,
       '{"auto_merge":95,"review":80}'::jsonb,
       TRUE
WHERE NOT EXISTS (SELECT 1 FROM md_match_config WHERE dict_code = 'gl_account' AND rule_name = '科目默认查重');




INSERT INTO md_match_config (id, rule_name, dict_code, target_table, specs, cluster_keys, survive_fields, thresholds, is_active)
SELECT 5, '币种默认查重', 'currency', 'cm_currency',
       '[{"field":"currency_code","weight":60,"kind":"Exact"},{"field":"name","weight":40,"kind":"EditDistance"}]'::jsonb,
       '["currency_code","name"]'::jsonb,
       '["name","symbol","decimal_places","is_base"]'::jsonb,
       '{"auto_merge":95,"review":80}'::jsonb,
       TRUE
WHERE NOT EXISTS (SELECT 1 FROM md_match_config WHERE dict_code = 'currency' AND rule_name = '币种默认查重');

INSERT INTO md_match_config (id, rule_name, dict_code, target_table, specs, cluster_keys, survive_fields, thresholds, is_active)
SELECT 6, '计量单位默认查重', 'uom', 'cm_uom',
       '[{"field":"uom_code","weight":60,"kind":"Exact"},{"field":"name","weight":40,"kind":"EditDistance"}]'::jsonb,
       '["uom_code","name"]'::jsonb,
       '["name","unit_type"]'::jsonb,
       '{"auto_merge":95,"review":80}'::jsonb,
       TRUE
WHERE NOT EXISTS (SELECT 1 FROM md_match_config WHERE dict_code = 'uom' AND rule_name = '计量单位默认查重');

INSERT INTO md_match_config (id, rule_name, dict_code, target_table, specs, cluster_keys, survive_fields, thresholds, is_active)
SELECT 7, '物料分类默认查重', 'material_class', 'cm_material_class',
       '[{"field":"class_code","weight":50,"kind":"Exact"},{"field":"name","weight":50,"kind":"EditDistance"}]'::jsonb,
       '["class_code","name"]'::jsonb,
       '["name","class_type"]'::jsonb,
       '{"auto_merge":95,"review":80}'::jsonb,
       TRUE
WHERE NOT EXISTS (SELECT 1 FROM md_match_config WHERE dict_code = 'material_class' AND rule_name = '物料分类默认查重');

INSERT INTO md_match_config (id, rule_name, dict_code, target_table, specs, cluster_keys, survive_fields, thresholds, is_active)
SELECT 8, '成本中心默认查重', 'cost_center', 'cm_cost_center',
       '[{"field":"cost_center_code","weight":50,"kind":"Exact"},{"field":"name","weight":50,"kind":"EditDistance"}]'::jsonb,
       '["cost_center_code","name"]'::jsonb,
       '["name","dept_id"]'::jsonb,
       '{"auto_merge":95,"review":80}'::jsonb,
       TRUE
WHERE NOT EXISTS (SELECT 1 FROM md_match_config WHERE dict_code = 'cost_center' AND rule_name = '成本中心默认查重');

INSERT INTO md_match_config (id, rule_name, dict_code, target_table, specs, cluster_keys, survive_fields, thresholds, is_active)
SELECT 9, '利润中心默认查重', 'profit_center', 'cm_profit_center',
       '[{"field":"profit_center_code","weight":50,"kind":"Exact"},{"field":"name","weight":50,"kind":"EditDistance"}]'::jsonb,
       '["profit_center_code","name"]'::jsonb,
       '["name"]'::jsonb,
       '{"auto_merge":95,"review":80}'::jsonb,
       TRUE
WHERE NOT EXISTS (SELECT 1 FROM md_match_config WHERE dict_code = 'profit_center' AND rule_name = '利润中心默认查重');

INSERT INTO md_match_config (id, rule_name, dict_code, target_table, specs, cluster_keys, survive_fields, thresholds, is_active)
SELECT 10, '公司默认查重', 'company', 'cm_company',
       '[{"field":"credit_code","weight":50,"kind":"Exact"},{"field":"name","weight":50,"kind":"EditDistance"}]'::jsonb,
       '["credit_code","name"]'::jsonb,
       '["name","short_name","credit_code","legal_person","base_currency_id","registered_address"]'::jsonb,
       '{"auto_merge":95,"review":80}'::jsonb,
       TRUE
WHERE NOT EXISTS (SELECT 1 FROM md_match_config WHERE dict_code = 'company' AND rule_name = '公司默认查重');

INSERT INTO md_match_config (id, rule_name, dict_code, target_table, specs, cluster_keys, survive_fields, thresholds, is_active)
SELECT 11, '组织默认查重', 'organization', 'cm_organization',
       '[{"field":"org_code","weight":50,"kind":"Exact"},{"field":"name","weight":50,"kind":"EditDistance"}]'::jsonb,
       '["org_code","name"]'::jsonb,
       '["name","company_id","org_type"]'::jsonb,
       '{"auto_merge":95,"review":80}'::jsonb,
       TRUE
WHERE NOT EXISTS (SELECT 1 FROM md_match_config WHERE dict_code = 'organization' AND rule_name = '组织默认查重');

INSERT INTO md_match_config (id, rule_name, dict_code, target_table, specs, cluster_keys, survive_fields, thresholds, is_active)
SELECT 12, '部门默认查重', 'department', 'cm_department',
       '[{"field":"dept_code","weight":50,"kind":"Exact"},{"field":"name","weight":50,"kind":"EditDistance"}]'::jsonb,
       '["dept_code","name"]'::jsonb,
       '["name","org_id"]'::jsonb,
       '{"auto_merge":95,"review":80}'::jsonb,
       TRUE
WHERE NOT EXISTS (SELECT 1 FROM md_match_config WHERE dict_code = 'department' AND rule_name = '部门默认查重');

INSERT INTO md_match_config (id, rule_name, dict_code, target_table, specs, cluster_keys, survive_fields, thresholds, is_active)
SELECT 13, '岗位默认查重', 'position', 'cm_position',
       '[{"field":"position_code","weight":60,"kind":"Exact"},{"field":"name","weight":40,"kind":"EditDistance"}]'::jsonb,
       '["position_code","name"]'::jsonb,
       '["name","job_family","job_grade"]'::jsonb,
       '{"auto_merge":95,"review":80}'::jsonb,
       TRUE
WHERE NOT EXISTS (SELECT 1 FROM md_match_config WHERE dict_code = 'position' AND rule_name = '岗位默认查重');

INSERT INTO md_match_config (id, rule_name, dict_code, target_table, specs, cluster_keys, survive_fields, thresholds, is_active)
SELECT 14, '员工默认查重', 'employee', 'cm_employee',
       '[{"field":"emp_no","weight":50,"kind":"Exact"},{"field":"name","weight":30,"kind":"EditDistance"},{"field":"mobile","weight":20,"kind":"Exact"}]'::jsonb,
       '["emp_no","name","mobile"]'::jsonb,
       '["name","mobile","email","company_id","dept_id","position_id","hire_date","emp_status"]'::jsonb,
       '{"auto_merge":95,"review":80}'::jsonb,
       TRUE
WHERE NOT EXISTS (SELECT 1 FROM md_match_config WHERE dict_code = 'employee' AND rule_name = '员工默认查重');


-- ============================================================
-- 4. 分发水位（md_dist_watermark）
-- ============================================================

INSERT INTO md_dist_watermark (key, last_seq) VALUES ('fanout', 0) ON CONFLICT (key) DO NOTHING;

-- ============================================================
-- 报表索引修正（原迁移 20260720_001_cr_report_sheet_multisheet）
-- cr_report_sheet 多 sheet 改造后原唯一索引不再成立，改部分唯一索引。
-- cr_* 表 DDL 由模型中心运行时部署：表未部署时静默跳过，部署后重放本基线即补上。
-- ============================================================
-- cr_report_sheet 允许「一报表 + 一版本」承载多个 sheet。
--
-- 背景：cr_report_sheet 早期定义遗留了一条 2 列唯一索引
--   uk_cr_report_sheet_1 UNIQUE (report_code, version_code)
-- 它把「同一报表同一版本只能有 1 个 sheet」写死。插入第 2 个 sheet 时必然报
--   duplicate key value violates unique constraint "uk_cr_report_sheet_1"
-- 表现为报表设计器「插入多 sheet 保存出错」。
--
-- 主键 PRIMARY KEY (report_code, version_code, sheet_index) 本就保证了 sheet 唯一性，
-- 且定义源 cmxfico_report_dct_meta_v1.json 的 uniqueKeys 已是 3 列
--   [["report_code","version_code","sheet_index"]]，
-- 故这条 2 列索引是过时孤儿，删除即可，DDL 同步不会再生成它。
DROP INDEX IF EXISTS uk_cr_report_sheet_1;
