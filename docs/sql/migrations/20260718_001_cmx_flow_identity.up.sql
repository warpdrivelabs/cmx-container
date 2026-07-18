-- cmx-flow 流程引擎 M4.1：组织/岗位（IAM 补齐）+ 任务候选人池
-- 幂等：CREATE TABLE / INDEX IF NOT EXISTS。
-- 约束遵循：cmx_ 前缀；无 FOREIGN KEY（关联字段 + 索引替代）；标准列风格对齐 cmx_role/cmx_user_role。
-- 依赖：既有 IAM（cmx_user / cmx_role / cmx_user_role）。
-- 涉及变更：
--   通用（非 flow 专属，补齐 IAM 缺口）：
--     cmx_org           —— 组织/部门树（补齐 cmx_user.org_id 的悬空引用）
--     cmx_position      —— 岗位表
--     cmx_user_position —— 用户-岗位关联（多对多，镜像 cmx_user_role）
--   flow 专属：
--     cmx_flow_task_candidate —— 任务候选人池（多人候选待认领）
-- 说明：办理人从静态字符串升级为候选人表达式 role()/position()/org()/user()，令牌到达时
--       由 PgIamAssigneeResolver 解析成真实用户：单人直派、多人落候选池待 claim。

-- =============================================
-- 1. 组织 / 部门树
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
-- 2. 岗位表
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
COMMENT ON TABLE  cmx_position       IS '岗位表（组织内职位，与角色正交）';
COMMENT ON COLUMN cmx_position.code  IS '岗位编码（唯一，候选人解析 position() 用）';
COMMENT ON COLUMN cmx_position.org_id IS '所属部门（可空=全局岗位）';
COMMENT ON COLUMN cmx_position.level IS '职级（审批层级/自动升级预留）';
CREATE UNIQUE INDEX IF NOT EXISTS uk_cmx_position_code ON cmx_position (code) WHERE archived = 0;
CREATE INDEX IF NOT EXISTS idx_cmx_position_org ON cmx_position (org_id);

-- =============================================
-- 3. 用户-岗位关联（多对多，镜像 cmx_user_role）
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
COMMENT ON TABLE  cmx_user_position             IS '用户-岗位关联表（一人可多岗）';
COMMENT ON COLUMN cmx_user_position.is_primary  IS '是否主岗';
CREATE INDEX IF NOT EXISTS idx_cmx_user_position_user ON cmx_user_position (user_id);
CREATE INDEX IF NOT EXISTS idx_cmx_user_position_pos  ON cmx_user_position (position_id);

-- =============================================
-- 4. 任务候选人池（flow 运行态）
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
