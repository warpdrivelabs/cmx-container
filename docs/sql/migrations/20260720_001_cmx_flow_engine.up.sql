-- =============================================
-- 迁移说明：cmx-flow 流程引擎完整建表（M1~M5.3 + 设计器阶段0 + 菜单注册）
--          合并自 20260717_001 ~ 20260718_011 共 14 个碎片化迁移，采用最终态 CREATE。
-- 影响表：cmx_flow_instance, cmx_flow_token, cmx_flow_task, cmx_flow_hi_instance,
--         cmx_flow_hi_task, cmx_flow_mi_scope, cmx_flow_job, cmx_org, cmx_position,
--         cmx_user_position, cmx_flow_task_candidate, cmx_flow_cc, cmx_flow_task_delegation,
--         cmx_flow_subflow_binding, cmx_flow_definition, cmx_flow_definition_version,
--         cmx_menu (INSERT)
-- 操作类型：CREATE TABLE / CREATE INDEX / INSERT
-- 回滚方式：20260720_001_cmx_flow_engine.down.sql
-- 约束遵循：cmx_ 前缀；无 FOREIGN KEY（关联字段 + 索引替代）；标准 TIMESTAMPTZ。
-- 说明：实例进入终态时由 cmx-flow-store-pg 在同事务内归档到 HI 表（幂等 upsert）。
--       RU/HI 分离对齐 Flowable 的运行态/历史态设计。
-- =============================================

-- =============================================
-- 1. 流程实例表（运行态聚合根；含 M5.1 子流程父子列 + M5.3 多挂载去重列）
-- =============================================
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
COMMENT ON TABLE  cmx_flow_instance                       IS '流程实例（运行态聚合根）';
COMMENT ON COLUMN cmx_flow_instance.definition_key        IS '流程定义 key（BPMN process id）';
COMMENT ON COLUMN cmx_flow_instance.business_key          IS '业务键，对接业务单据（可空）';
COMMENT ON COLUMN cmx_flow_instance.state                 IS '实例状态：ACTIVE / COMPLETED / TERMINATED';
COMMENT ON COLUMN cmx_flow_instance.variables             IS '实例级流程变量（JSONB 动态 KV）';
COMMENT ON COLUMN cmx_flow_instance.org_id                IS '所属组织（M5.2 子流程组织路由依据；M5.1 恒空）';
COMMENT ON COLUMN cmx_flow_instance.parent_instance_id    IS '父实例 id（子实例指向主实例；主实例为 NULL）';
COMMENT ON COLUMN cmx_flow_instance.parent_token_id       IS '父实例中挂起等待的令牌 id（子完成时据此精确唤醒）';
COMMENT ON COLUMN cmx_flow_instance.parent_node_bpmn_id   IS '父实例中发起本子实例的 callActivity 节点 bpmn id（M5.3 多挂载去重键；单挂载恒空）';
CREATE INDEX IF NOT EXISTS idx_cmx_flow_instance_defkey ON cmx_flow_instance (definition_key);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_instance_bizkey ON cmx_flow_instance (business_key);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_instance_state  ON cmx_flow_instance (state);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_instance_parent ON cmx_flow_instance (parent_instance_id);

-- =============================================
-- 2. 令牌表（执行指针）
-- =============================================
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
COMMENT ON TABLE  cmx_flow_token              IS '流程令牌（执行指针；一实例多令牌，无外键关联 instance）';
COMMENT ON COLUMN cmx_flow_token.instance_id  IS '所属实例 id（逻辑关联 cmx_flow_instance.id）';
COMMENT ON COLUMN cmx_flow_token.node_bpmn_id IS '当前所在节点的 BPMN id（稳定锚点）';
COMMENT ON COLUMN cmx_flow_token.state        IS '令牌状态：ACTIVE / WAITING / JOINING / WAITING_SUBFLOW / ENDED';
COMMENT ON COLUMN cmx_flow_token.parent_id    IS '父令牌 id（并行网关 fork 分裂血缘；可空）';
CREATE INDEX IF NOT EXISTS idx_cmx_flow_token_instance ON cmx_flow_token (instance_id);

-- =============================================
-- 3. 用户任务表（含 M3 多实例 element_value + M4.3 转签三列）
-- =============================================
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
COMMENT ON TABLE  cmx_flow_task                  IS '用户任务（userTask 等待态的外化产物）';
COMMENT ON COLUMN cmx_flow_task.instance_id      IS '所属实例 id（逻辑关联 cmx_flow_instance.id）';
COMMENT ON COLUMN cmx_flow_task.token_id         IS '产生该任务的令牌 id（逻辑关联 cmx_flow_token.id）';
COMMENT ON COLUMN cmx_flow_task.node_bpmn_id     IS '对应 userTask 节点的 BPMN id';
COMMENT ON COLUMN cmx_flow_task.candidate_groups IS '候选组（逗号分隔，M2 未解析）';
COMMENT ON COLUMN cmx_flow_task.element_value    IS '多实例子任务携带的当前元素（会签每人各自数据；单实例任务为 NULL）';
COMMENT ON COLUMN cmx_flow_task.owner_user_id    IS '任务所有者（委派时 ≠ assignee；None=owner即assignee）';
COMMENT ON COLUMN cmx_flow_task.parent_task_id   IS '父任务 id（加签临时任务指向原任务；主任务为 NULL）';
COMMENT ON COLUMN cmx_flow_task.delegation_state IS 'NULL=常规 / DELEGATED / ADDSIGN(临时) / SUSPENDED(被加签挂起)';
CREATE INDEX IF NOT EXISTS idx_cmx_flow_task_instance ON cmx_flow_task (instance_id);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_task_assignee ON cmx_flow_task (assignee);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_task_open     ON cmx_flow_task (assignee, completed);

-- =============================================
-- 4. 历史实例表（HI：终态归档，与热运行态解耦）
-- =============================================
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
COMMENT ON COLUMN cmx_flow_hi_instance.duration_ms IS '实例存续时长（ended_at - created_at，毫秒）';
COMMENT ON COLUMN cmx_flow_hi_instance.archived_at IS '归档写入时刻';
CREATE INDEX IF NOT EXISTS idx_cmx_flow_hi_instance_defkey ON cmx_flow_hi_instance (definition_key);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_hi_instance_bizkey ON cmx_flow_hi_instance (business_key);

-- =============================================
-- 5. 历史任务表（HI：办结任务归档，含耗时）
-- =============================================
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
COMMENT ON COLUMN cmx_flow_hi_task.instance_id IS '所属实例 id（逻辑关联 cmx_flow_hi_instance.id）';
COMMENT ON COLUMN cmx_flow_hi_task.duration_ms IS '任务办理时长（completed_at - created_at，毫秒）';
COMMENT ON COLUMN cmx_flow_hi_task.archived_at IS '归档写入时刻';
CREATE INDEX IF NOT EXISTS idx_cmx_flow_hi_task_instance ON cmx_flow_hi_task (instance_id);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_hi_task_assignee ON cmx_flow_hi_task (assignee);

-- =============================================
-- 6. 多实例执行域（M3：会签/或签账本）
-- =============================================
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
COMMENT ON TABLE  cmx_flow_mi_scope                      IS '多实例执行域（会签/或签的一次展开：计数与游标账本）';
COMMENT ON COLUMN cmx_flow_mi_scope.instance_id          IS '所属实例 id（逻辑关联 cmx_flow_instance.id）';
COMMENT ON COLUMN cmx_flow_mi_scope.node_bpmn_id         IS '对应 multiInstance 节点的 BPMN id';
COMMENT ON COLUMN cmx_flow_mi_scope.sequential           IS 'true=顺序(或签，逐个办理)；false=并行(会签，齐头并进)';
COMMENT ON COLUMN cmx_flow_mi_scope.total                IS '展开的子实例总数（nrOfInstances）';
COMMENT ON COLUMN cmx_flow_mi_scope.completed            IS '已办结的子实例数（nrOfCompletedInstances）';
COMMENT ON COLUMN cmx_flow_mi_scope.next_index           IS '顺序模式下一个待展开元素下标；并行模式恒等于 total';
COMMENT ON COLUMN cmx_flow_mi_scope.collection           IS '展开用的元素快照（JSONB 数组，定格避免中途变量被改）';
COMMENT ON COLUMN cmx_flow_mi_scope.element_var          IS '子任务携带当前元素的变量名（elementVariable，可空）';
COMMENT ON COLUMN cmx_flow_mi_scope.completion_condition IS '完成条件表达式（可空；命中即提前收口剩余子实例）';
COMMENT ON COLUMN cmx_flow_mi_scope.finished             IS '本域是否已收口（完成条件命中或自然全部完成）';
CREATE INDEX IF NOT EXISTS idx_cmx_flow_mi_scope_instance ON cmx_flow_mi_scope (instance_id);

-- =============================================
-- 7. 定时器作业（M2.5：边界定时器到期表）
-- =============================================
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
COMMENT ON TABLE  cmx_flow_job                  IS '定时器作业（边界定时器「到期待触发」表）';
COMMENT ON COLUMN cmx_flow_job.instance_id      IS '所属实例 id（逻辑关联 cmx_flow_instance.id）';
COMMENT ON COLUMN cmx_flow_job.token_id         IS '挂载该定时器的令牌 id（停在宿主 userTask）；令牌离开即撤销本作业';
COMMENT ON COLUMN cmx_flow_job.boundary_bpmn_id IS '触发时令牌要去的边界事件节点 bpmn_id';
COMMENT ON COLUMN cmx_flow_job.cancel_activity  IS 'true=中断型(超时中断宿主任务)；false=非中断型(发旁路令牌，宿主不断)';
COMMENT ON COLUMN cmx_flow_job.due_at           IS '到期时刻（宿主到达时刻 + 时长）';
CREATE INDEX IF NOT EXISTS idx_cmx_flow_job_instance ON cmx_flow_job (instance_id);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_job_due      ON cmx_flow_job (due_at);

-- =============================================
-- 8. 组织/部门树（M4.1：IAM 补齐，补齐 cmx_user.org_id 引用）
-- =============================================
CREATE TABLE IF NOT EXISTS cmx_org
(
    id             VARCHAR(64)  NOT NULL,
    code           VARCHAR(100) NOT NULL,
    name           VARCHAR(100) NOT NULL,
    parent_id      VARCHAR(64),
    path           VARCHAR(500),
    leader_user_id VARCHAR(64),
    sort_order     INT4      DEFAULT 0,
    status         INT4      DEFAULT 1,
    archived       INT4      DEFAULT 0,
    create_time    TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    update_time    TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (id)
);
COMMENT ON TABLE  cmx_org                IS '组织/部门表（树形；补齐 cmx_user.org_id 引用）';
COMMENT ON COLUMN cmx_org.code           IS '部门编码（唯一，候选人解析 org() 用）';
COMMENT ON COLUMN cmx_org.parent_id      IS '父部门 id（NULL=根）';
COMMENT ON COLUMN cmx_org.path           IS '物化路径（如 /d_root/d_fin，子树前缀查询用）';
COMMENT ON COLUMN cmx_org.leader_user_id IS '部门负责人 user id（org.leader 解析预留）';
CREATE UNIQUE INDEX IF NOT EXISTS uk_cmx_org_code ON cmx_org (code) WHERE archived = 0;
CREATE INDEX IF NOT EXISTS idx_cmx_org_parent ON cmx_org (parent_id);

-- =============================================
-- 9. 岗位表（M4.1：与角色正交）
-- =============================================
CREATE TABLE IF NOT EXISTS cmx_position
(
    id          VARCHAR(64)  NOT NULL,
    code        VARCHAR(100) NOT NULL,
    name        VARCHAR(100) NOT NULL,
    org_id      VARCHAR(64),
    level       INT4      DEFAULT 0,
    sort_order  INT4      DEFAULT 0,
    status      INT4      DEFAULT 1,
    archived    INT4      DEFAULT 0,
    create_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    update_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (id)
);
COMMENT ON TABLE  cmx_position        IS '岗位表（组织内职位，与角色正交）';
COMMENT ON COLUMN cmx_position.code   IS '岗位编码（唯一，候选人解析 position() 用）';
COMMENT ON COLUMN cmx_position.org_id IS '所属部门（可空=全局岗位）';
COMMENT ON COLUMN cmx_position.level  IS '职级（审批层级/自动升级预留）';
CREATE UNIQUE INDEX IF NOT EXISTS uk_cmx_position_code ON cmx_position (code) WHERE archived = 0;
CREATE INDEX IF NOT EXISTS idx_cmx_position_org ON cmx_position (org_id);

-- =============================================
-- 10. 用户-岗位关联（M4.1：多对多，镜像 cmx_user_role）
-- =============================================
CREATE TABLE IF NOT EXISTS cmx_user_position
(
    id          VARCHAR(64) NOT NULL,
    user_id     VARCHAR(64) NOT NULL,
    position_id VARCHAR(64) NOT NULL,
    is_primary  BOOLEAN   DEFAULT FALSE,
    archived    INT4      DEFAULT 0,
    create_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    update_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (id)
);
COMMENT ON TABLE  cmx_user_position            IS '用户-岗位关联表（一人可多岗）';
COMMENT ON COLUMN cmx_user_position.is_primary IS '是否主岗';
CREATE INDEX IF NOT EXISTS idx_cmx_user_position_user ON cmx_user_position (user_id);
CREATE INDEX IF NOT EXISTS idx_cmx_user_position_pos  ON cmx_user_position (position_id);

-- =============================================
-- 11. 任务候选人池（M4.1：多人候选待认领）
-- =============================================
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
COMMENT ON TABLE  cmx_flow_task_candidate                  IS '任务候选人池（多人候选待认领；随快照全删重插）';
COMMENT ON COLUMN cmx_flow_task_candidate.candidate_type   IS '候选来源：USER / ROLE / POSITION / ORG';
COMMENT ON COLUMN cmx_flow_task_candidate.candidate_ref    IS '候选引用原值（role code / position code / org id / user id）';
COMMENT ON COLUMN cmx_flow_task_candidate.resolved_user_id IS '解析出的具体用户 id（供"我的待办"按用户查）';
CREATE INDEX IF NOT EXISTS idx_cmx_flow_task_candidate_instance ON cmx_flow_task_candidate (instance_id);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_task_candidate_user     ON cmx_flow_task_candidate (resolved_user_id);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_task_candidate_task     ON cmx_flow_task_candidate (task_id);

-- =============================================
-- 12. 抄送记录（M4.2：只读知会 + 已读追踪；不阻塞流程）
-- =============================================
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

-- =============================================
-- 13. 转签台账（M4.3：转办/加签/委派流转链）
-- =============================================
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
COMMENT ON TABLE  cmx_flow_task_delegation              IS '转签台账（转办/加签/委派流转链，供审计与展示）';
COMMENT ON COLUMN cmx_flow_task_delegation.kind         IS 'TRANSFER / ADDSIGN_BEFORE / ADDSIGN_AFTER / DELEGATE';
COMMENT ON COLUMN cmx_flow_task_delegation.temp_task_id IS '加签产生的临时任务 id（转办/委派为 NULL）';
CREATE INDEX IF NOT EXISTS idx_cmx_flow_task_delegation_instance ON cmx_flow_task_delegation (instance_id);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_task_delegation_task     ON cmx_flow_task_delegation (task_id);

-- =============================================
-- 14. 子流程组织绑定（M5.2：逻辑 key + 组织 -> 具体子流程）
--     原 20260718_005（demo 库）与 20260718_011（生产库补建）合并；
--     此表须在 IAM/主库（primary = cmx）存在，PgSubflowRouter / PgSubflowBindingStore 都指 IAM_DB_ID。
-- =============================================
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
COMMENT ON TABLE  cmx_flow_subflow_binding                       IS '子流程组织绑定（逻辑 key + 组织 -> 具体子流程定义；定义态配置，运行期 SubflowRouter 解析）';
COMMENT ON COLUMN cmx_flow_subflow_binding.called_key            IS 'callActivity 的 cmx:calledKey（逻辑子流程名）';
COMMENT ON COLUMN cmx_flow_subflow_binding.org_id                IS '适用组织（NULL = 默认兜底绑定）';
COMMENT ON COLUMN cmx_flow_subflow_binding.target_definition_key IS '解析到的具体子流程定义 key';
COMMENT ON COLUMN cmx_flow_subflow_binding.enabled              IS '是否启用（FALSE 不参与解析）';
CREATE INDEX IF NOT EXISTS idx_cmx_flow_subflow_binding_key ON cmx_flow_subflow_binding (called_key);
CREATE INDEX IF NOT EXISTS idx_cmx_flow_subflow_binding_org ON cmx_flow_subflow_binding (org_id);

-- =============================================
-- 15. 流程定义主记录（设计器阶段0；当前指针：草稿 XML + 已发布版本指向）
--     含 DAM 三段列（domain/application/module）
-- =============================================
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

-- =============================================
-- 16. 流程定义版本历史（不可变，每次发布追加 BPMN 快照；含 note 变更说明列）
-- =============================================
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

-- =============================================
-- 17. 流程设计工作台菜单注册（对标报表设计工作台 fi-gl-rpt-design-workbench）
--     同源已加入 data/menu-pages/fi/cmxfico/gl/explorer-menu.json，menu-generator 重跑不丢
-- =============================================
INSERT INTO cmx_menu (id, code, name, icon, fun_code, sort_order, definition, domain_code, application_code, module_code, parent_id, parent_code, depth, leaf, code_path, id_path, visible, status, open_type, archived, create_time, update_time) VALUES ('7484207786453110900', 'fi-gl-flow-design-workbench', '流程设计工作台', 'workflow-tasks', NULL, 31, '{"caption":"流程设计工作台","workspace":{"id":"flow_design_workbench","explorer":{"caption":"流程定义","icon":"tree","views":[{"id":"flow-design-workbench-explorer","tabLabel":"定义","icon":"tree","type":"native_pages","native_page":"portal.flow.design-workbench","view":"explorer"}]},"content":{"caption":"流程设计工作台","icon":"workflow-tasks","views":[{"id":"flow-design-workbench-content","tabLabel":"流程图","icon":"workflow-tasks","type":"native_pages","native_page":"portal.flow.design-workbench","view":"content"}]},"property":{"caption":"节点属性","icon":"detail-view","views":[{"id":"flow-design-workbench-prop","tabLabel":"属性","icon":"detail-view","type":"native_pages","native_page":"portal.flow.design-workbench","view":"property"}]}},"type":"workspace-node","name":"flow-design-workbench"}'::jsonb, 'fi', 'cmxfico', 'gl', '7484207786453110784', 'gl', 2, 1, '/gl/fi-gl-flow-design-workbench', '/7484207786453110784/7484207786453110900', 1, 1, 0, 0, now(), now()) ON CONFLICT (code) WHERE archived = 0 DO NOTHING;
