-- ============================================================
-- CMX 业务库全量 DDL — docs/sql/v2/biz/init_ddl.sql
--
-- 目标库：业务数据源（source_type = "biz"）
-- 归属规则：非 cmx_ 业务表 + 三组 cmx_ 例外建业务库：
--   · md_*  11 张 —— MDM 治理表
--   · mdm_activation —— MDM 激活映射（原 cmx_mdm_activation，归业务库侧）
--   · cmx_code_* 3 张 —— 编码引擎（rule/gap/seq；运行时 code API 经
--     resolve_db_id 回退业务库）
--   · cmx_flow_* 15 张 —— 流程运行态（与流程引擎 FLOW_DB_ID=业务库一致；
--     IAM 侧 cmx_org/cmx_position/cmx_user_position 留主库，见 ../platform/）
--   · cmx_doc_revision / cmx_doc_change 2 张 —— 业务单据版本化（整单快照 +
--     字段级变更明细；模型中心 DOC 存储运行时写入；20260827 自主库迁入）
-- 风格：表定义即终态（无 ALTER）；无损幂等；每表区块：CREATE TABLE → COMMENT → 索引
-- 面向：新库手工重建与结构参考；存量库升级走 migrations 基线迁移
-- 注意：cf_*/cr_*/cm_* 等业务表 DDL 由模型中心/插件运行时部署，不在本文件
--       （种子见 seeds/，表部署后手工执行）
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
    version             BIGINT       NOT NULL DEFAULT 0,
    system_id           VARCHAR(64),
    PRIMARY KEY (id)
);
COMMENT ON TABLE  cmx_flow_instance                    IS '流程实例（运行态聚合根）';
COMMENT ON COLUMN cmx_flow_instance.state              IS '实例状态：ACTIVE / COMPLETED / TERMINATED';
COMMENT ON COLUMN cmx_flow_instance.variables          IS '实例级流程变量（JSONB 动态 KV）';
COMMENT ON COLUMN cmx_flow_instance.parent_instance_id IS '父实例 id（M5 子流程：子实例指向主实例；主实例为 NULL）';
COMMENT ON COLUMN cmx_flow_instance.parent_token_id    IS '父实例中挂起等待的令牌 id（子完成时精确唤醒）';
COMMENT ON COLUMN cmx_flow_instance.parent_node_bpmn_id IS '父实例中发起本子实例的 callActivity 节点 bpmn id（M5.3 多挂载去重键；单挂载恒空）';
COMMENT ON COLUMN cmx_flow_instance.version             IS '乐观锁版本（技术债 007）：save 以 WHERE id AND version CAS 提交并 +1，0 行即并发冲突 409';
COMMENT ON COLUMN cmx_flow_instance.system_id           IS '发起方业务系统标识（技术债 005：来自结构化 API Key 声明；NULL = legacy 调用未声明系统）；子实例继承';
CREATE INDEX IF NOT EXISTS idx_cmx_flow_instance_defkey ON cmx_flow_instance (definition_key);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_instance_bizkey ON cmx_flow_instance (business_key);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_instance_state  ON cmx_flow_instance (state);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_instance_parent ON cmx_flow_instance (parent_instance_id);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_instance_system  ON cmx_flow_instance (system_id);

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
    version        BIGINT       NOT NULL DEFAULT 0,
    system_id      VARCHAR(64),
    PRIMARY KEY (id)
);
COMMENT ON TABLE  cmx_flow_hi_instance             IS '历史流程实例（终态归档，供审计/查询）';
COMMENT ON COLUMN cmx_flow_hi_instance.duration_ms IS '实例存续时长（毫秒）';
COMMENT ON COLUMN cmx_flow_hi_instance.version      IS '归档时的乐观锁版本（技术债 007 审计留档）';
COMMENT ON COLUMN cmx_flow_hi_instance.system_id    IS '发起方业务系统标识（技术债 005 归档登记）';
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
    claimed_by       VARCHAR(128),
    lease_expires_at TIMESTAMPTZ,
    PRIMARY KEY (id)
);
COMMENT ON TABLE  cmx_flow_job                 IS '定时器作业（边界定时器到期表）';
COMMENT ON COLUMN cmx_flow_job.claimed_by       IS '定时器抢占持有者（技术债 008；worker id = timer-<pid>）';
COMMENT ON COLUMN cmx_flow_job.lease_expires_at IS '租约到期时刻（到期后作业可被其它副本重抢）';
COMMENT ON COLUMN cmx_flow_job.token_id        IS '挂载令牌 id（停在宿主 userTask）；令牌离开即撤销作业';
COMMENT ON COLUMN cmx_flow_job.cancel_activity IS 'true=中断型；false=非中断型';
COMMENT ON COLUMN cmx_flow_job.due_at          IS '到期时刻（宿主到达 + 时长）';
CREATE INDEX IF NOT EXISTS idx_cmx_flow_job_instance ON cmx_flow_job (instance_id);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_job_due      ON cmx_flow_job (due_at);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_job_acquire  ON cmx_flow_job (due_at, claimed_by, lease_expires_at);

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
    group_id       BIGINT,
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
COMMENT ON COLUMN cmx_flow_definition.group_id       IS '所属流程分组 id → cmx_flow_def_group.id（NULL = 未分组；订阅规则 groupIds 匹配维度）';
CREATE INDEX IF NOT EXISTS idx_cmx_flow_definition_module ON cmx_flow_definition (module);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_definition_state  ON cmx_flow_definition (state);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_definition_dam    ON cmx_flow_definition (domain, application, module);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_definition_group  ON cmx_flow_definition (group_id);

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

-- 流程定义分组（一级扁平；定义列表页左侧面板归属维度 + 订阅规则 groupIds 匹配维度）
CREATE TABLE IF NOT EXISTS cmx_flow_def_group (
    id         BIGINT       NOT NULL,
    name       VARCHAR(64)  NOT NULL,
    sort_no    INT          NOT NULL DEFAULT 0,
    enabled    BOOLEAN      NOT NULL DEFAULT TRUE,
    remark     VARCHAR(512),
    created_at TIMESTAMPTZ  NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ  NOT NULL DEFAULT now(),
    PRIMARY KEY (id)
);
COMMENT ON TABLE  cmx_flow_def_group            IS '流程定义分组：一级扁平（非树）；enabled 仅影响定义页展示位，永不参与运行时匹配';
COMMENT ON COLUMN cmx_flow_def_group.name       IS '分组名（全局唯一）';
COMMENT ON COLUMN cmx_flow_def_group.sort_no    IS '展示序（定义页上移下移改此列；纯展示）';
COMMENT ON COLUMN cmx_flow_def_group.enabled    IS '启用位（仅定义页展示位：停用组折叠置灰）';
COMMENT ON COLUMN cmx_flow_def_group.updated_at IS '最近更新时间（DB 时钟；进 EventRouteCache 指纹对账）';
CREATE UNIQUE INDEX IF NOT EXISTS uq_cmx_flow_def_group_name ON cmx_flow_def_group (name);

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
    user_name    VARCHAR(128),
    nick_name    VARCHAR(128),
    decision     VARCHAR(32),
    comment      TEXT,
    created_at   TIMESTAMPTZ  NOT NULL
);
COMMENT ON TABLE  cmx_flow_task_comment          IS '审批意见留痕（F3；办结时按环节记，供表单审批区展示历史）';
COMMENT ON COLUMN cmx_flow_task_comment.user_id  IS '办理人（谁办结/审批的，用户 id）';
COMMENT ON COLUMN cmx_flow_task_comment.user_name IS '办理人用户名快照（写入时点 username 口径展示名）';
COMMENT ON COLUMN cmx_flow_task_comment.nick_name IS '办理人昵称快照（写入时点 nickname 优先、username 兜底）';
COMMENT ON COLUMN cmx_flow_task_comment.decision IS '决策：approve / reject 等';
CREATE INDEX IF NOT EXISTS idx_cmx_flow_task_comment_instance ON cmx_flow_task_comment (instance_id);

-- ============================================================
-- 业务单据版本化（DOC 单据版本审计）：整单快照 + 字段级变更明细
-- 原在 ../platform/ 42/43 号区块，20260827 迁入业务库。
-- append-only：不更新旧行，回滚/换版一律追加新版本记录；
-- 运行时 cmx-model cmx-doc-store-pg DocRevision 写入（FOR UPDATE 防并发）。
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
-- 事件订阅 + 持久化投递队列（cmx-flowengine 20260902 重构方案）
-- 订阅者 + rules JSONB 内嵌（逻辑两级、物理单行）；主键 = BIGINT 应用层 Pk52 雪花
-- （cmx-utils next_pk_id()，52 位 JS 安全）；投递表保序键独立为 seq BIGSERIAL（DB 提交序）。
-- 引擎侧幂等自举 DDL 同源维护（cmx-flow-app/src/event_store.rs）。
-- ============================================================

-- 事件订阅者表（注册回调方 + 多条订阅规则内嵌）
CREATE TABLE IF NOT EXISTS cmx_flow_event_subscriber (
    id             BIGINT        NOT NULL,
    name           VARCHAR(128)  NOT NULL,
    description    VARCHAR(512),
    channel        VARCHAR(16)   NOT NULL DEFAULT 'webhook',
    channel_config JSONB         NOT NULL DEFAULT '{}',
    rules          JSONB         NOT NULL DEFAULT '[]',
    retry_max      INT           NOT NULL DEFAULT 10,
    active         BOOLEAN       NOT NULL DEFAULT TRUE,
    tenant_id      VARCHAR(64)   NOT NULL DEFAULT 'default',
    created_by     VARCHAR(64),
    created_at     TIMESTAMPTZ   NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ   NOT NULL DEFAULT now(),
    PRIMARY KEY (id)
);
COMMENT ON TABLE  cmx_flow_event_subscriber                IS '事件订阅者：注册回调方 + 订阅规则 rules JSONB 内嵌（save 单行 upsert 天然原子，数组序即命中序）';
COMMENT ON COLUMN cmx_flow_event_subscriber.name           IS '订阅者名（uk：同租户唯一）';
COMMENT ON COLUMN cmx_flow_event_subscriber.channel        IS '通道类型：webhook（kafka/rabbitmq feature 预留，save 时校验注册表已注册）';
COMMENT ON COLUMN cmx_flow_event_subscriber.channel_config IS '通道配置（开放对象）：webhook {service_key, callback_path, secret——secret 明文，API 掩码回显}；MQ 只存路由目标，连接凭据走 toml';
COMMENT ON COLUMN cmx_flow_event_subscriber.rules          IS '订阅规则数组（元素 {name≤64 同订阅者内唯一, enabled, eventTypes[], groupIds[], keyPatterns[]}）：规则内三维 AND、跨规则 OR、数组序=命中序；全空规则=匹配全部（网关形态）';
COMMENT ON COLUMN cmx_flow_event_subscriber.retry_max      IS '最大尝试次数（含首发，对齐 mdm 口径）；默认 10 = 重试 9 次';
COMMENT ON COLUMN cmx_flow_event_subscriber.active         IS '启停：停用即不再生成投递行（存量行保留可查可清）';
COMMENT ON COLUMN cmx_flow_event_subscriber.tenant_id      IS '租户（db-per-tenant 下冗余登记）';
COMMENT ON COLUMN cmx_flow_event_subscriber.updated_at     IS '最近更新时间（DB 时钟；缓存指纹对账列——set-active 等 UPDATE 必带 now()）';
CREATE UNIQUE INDEX IF NOT EXISTS uq_cmx_flow_event_sub_name ON cmx_flow_event_subscriber (tenant_id, name);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_event_sub_upd ON cmx_flow_event_subscriber (updated_at);

-- 持久化投递队列 + 死信一体（租约抢占 + 同订阅者保序）
CREATE TABLE IF NOT EXISTS cmx_flow_event_delivery (
    id                    BIGINT        NOT NULL,
    seq                   BIGSERIAL     NOT NULL,
    subscriber_id         BIGINT        NOT NULL,
    subscriber_name       VARCHAR(128)  NOT NULL,
    channel               VARCHAR(16)   NOT NULL,
    event_id              VARCHAR(64)   NOT NULL,
    delivery_id           VARCHAR(160)  NOT NULL,
    source                VARCHAR(8)    NOT NULL DEFAULT 'emit',
    event_type            VARCHAR(32)   NOT NULL,
    definition_key        VARCHAR(128),
    business_key          VARCHAR(128),
    instance_id           VARCHAR(64)   NOT NULL,
    payload               JSONB         NOT NULL,
    state                 VARCHAR(16)   NOT NULL DEFAULT 'PENDING',
    attempts              INT           NOT NULL DEFAULT 0,
    next_attempt_at       TIMESTAMPTZ,
    locked_by             VARCHAR(64),
    lock_expires_at       TIMESTAMPTZ,
    last_error            TEXT,
    last_http_status      INT,
    last_response_snippet VARCHAR(512),
    matched_rule          VARCHAR(64),
    created_at            TIMESTAMPTZ   NOT NULL DEFAULT now(),
    delivered_at          TIMESTAMPTZ,
    PRIMARY KEY (id)
);
COMMENT ON TABLE  cmx_flow_event_delivery                     IS '事件持久化投递队列（事件×订阅者×命中规则一行；PENDING/IN_FLIGHT/DONE/DEAD/SKIPPED 状态机 + 死信）';
COMMENT ON COLUMN cmx_flow_event_delivery.seq                 IS '保序键（BIGSERIAL，DB 按提交序赋值；同订阅者按 seq 严格保序，雪花不做保序）';
COMMENT ON COLUMN cmx_flow_event_delivery.subscriber_id       IS '归属订阅者 id → cmx_flow_event_subscriber.id';
COMMENT ON COLUMN cmx_flow_event_delivery.subscriber_name     IS '订阅者名快照（写入时定版；订阅者删除/改名后流水仍可辨识）';
COMMENT ON COLUMN cmx_flow_event_delivery.channel             IS '通道快照（写入时定版）';
COMMENT ON COLUMN cmx_flow_event_delivery.event_id            IS '事件唯一键（emit 时 uuid；rebuild 确定性 rb- 前缀）；uk(订阅者,事件) 幂等仅约束 rebuild/test 重复点击';
COMMENT ON COLUMN cmx_flow_event_delivery.delivery_id         IS 'wire 幂等参考键 {instanceId}-{taskId?}-{occurredAt}（x-cmx-flow-delivery 头；索引非唯一）';
COMMENT ON COLUMN cmx_flow_event_delivery.source              IS '来源：emit 业务事件 / test 测试 / rebuild 补发（test 直达终态不参与保序）';
COMMENT ON COLUMN cmx_flow_event_delivery.matched_rule        IS '命中规则名快照（emit 时定版，列宽与规则名校验上限同为 64；test/rebuild 行 NULL）';
COMMENT ON COLUMN cmx_flow_event_delivery.state               IS '状态：PENDING 待投 / IN_FLIGHT 投递中 / DONE 成功 / DEAD 死信 / SKIPPED 人工处置；终态均不阻塞同订阅者保序';
COMMENT ON COLUMN cmx_flow_event_delivery.attempts            IS '已尝试次数（claim 时 +1；retry_max 含首发）';
COMMENT ON COLUMN cmx_flow_event_delivery.next_attempt_at     IS '退避到期时间（1s 起指数封顶 5min）';
COMMENT ON COLUMN cmx_flow_event_delivery.locked_by           IS '租约持有者（worker id；多副本投递互斥）';
COMMENT ON COLUMN cmx_flow_event_delivery.lock_expires_at     IS '租约到期（120s；逐行续租，过期可被重抢自愈）';
COMMENT ON COLUMN cmx_flow_event_delivery.last_error          IS '最近失败原因（死信诊断）';
COMMENT ON COLUMN cmx_flow_event_delivery.last_http_status    IS '最近 HTTP 状态码（死信诊断）';
COMMENT ON COLUMN cmx_flow_event_delivery.last_response_snippet IS '响应摘要（截断 512，死信诊断）';
COMMENT ON COLUMN cmx_flow_event_delivery.created_at          IS '创建时间（stats 时间窗扫描索引列）';
COMMENT ON COLUMN cmx_flow_event_delivery.delivered_at        IS '投递成功时间';
CREATE UNIQUE INDEX IF NOT EXISTS uq_cmx_flow_event_dlv_seq       ON cmx_flow_event_delivery (seq);
CREATE UNIQUE INDEX IF NOT EXISTS uq_cmx_flow_event_dlv_sub_event ON cmx_flow_event_delivery (subscriber_id, event_id);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_event_dlv_due    ON cmx_flow_event_delivery (state, next_attempt_at);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_event_dlv_sub    ON cmx_flow_event_delivery (subscriber_id, seq);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_event_dlv_did    ON cmx_flow_event_delivery (delivery_id);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_event_dlv_created ON cmx_flow_event_delivery (created_at);

-- 故障清单（技术债 011：跨实例 incident 台账；/incidents 端点 + 自动重试数据源）
CREATE TABLE IF NOT EXISTS cmx_flow_incident
(
    id             VARCHAR(64)  NOT NULL,
    instance_id    VARCHAR(64)  NOT NULL,
    token_id       VARCHAR(64),
    node_bpmn_id   VARCHAR(128) NOT NULL,
    definition_key VARCHAR(128) NOT NULL,
    business_key   VARCHAR(128),
    reason         TEXT         NOT NULL DEFAULT '',
    retries        INTEGER      NOT NULL DEFAULT 0,
    state          VARCHAR(16)  NOT NULL DEFAULT 'OPEN',
    created_at     TIMESTAMPTZ  NOT NULL,
    updated_at     TIMESTAMPTZ  NOT NULL,
    PRIMARY KEY (id)
);
COMMENT ON TABLE  cmx_flow_incident           IS '流程故障清单（技术债 011；实例变量 __incident 仍是实例内派生视图）';
COMMENT ON COLUMN cmx_flow_incident.state     IS 'OPEN / RESOLVED（retry_incident 成功后批量关闭）';
COMMENT ON COLUMN cmx_flow_incident.retries   IS '累计发生/重试次数（同 instance+node 幂等累加）';
CREATE UNIQUE INDEX IF NOT EXISTS uq_cmx_flow_incident_inst_node ON cmx_flow_incident (instance_id, node_bpmn_id);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_incident_state ON cmx_flow_incident (state, updated_at);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_incident_def   ON cmx_flow_incident (definition_key);
