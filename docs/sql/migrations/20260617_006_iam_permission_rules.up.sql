-- =====================================================
-- cmx-iam 权限完善 - 阶段 1：临时角色/权限授予
-- 新增表：cmx_user_role_assignment（用户角色临时授权表）
-- =====================================================

-- 用户角色临时授权表
DROP TABLE IF EXISTS cmx_user_role_assignment;
CREATE TABLE cmx_user_role_assignment (
    id varchar(64) NOT NULL,
    user_id varchar(64) NOT NULL,
    role_id varchar(64) NOT NULL,
    effective_from timestamp NOT NULL,
    effective_until timestamp NOT NULL,
    reason varchar(500),
    source varchar(20) DEFAULT 'manual',
    status int4 DEFAULT 1,
    revoked_by varchar(100),
    revoked_at timestamp,
    archived int4 DEFAULT 0,
    create_time timestamp DEFAULT CURRENT_TIMESTAMP,
    update_time timestamp DEFAULT CURRENT_TIMESTAMP,
    create_by varchar(100),
    create_name varchar(100),
    update_by varchar(100),
    update_name varchar(100),
    CONSTRAINT pk_cmx_user_role_assignment PRIMARY KEY (id)
);

CREATE INDEX idx_cmx_user_role_assignment_user ON cmx_user_role_assignment (user_id);
CREATE INDEX idx_cmx_user_role_assignment_role ON cmx_user_role_assignment (role_id);
CREATE INDEX idx_cmx_user_role_assignment_time ON cmx_user_role_assignment (effective_from, effective_until);
CREATE INDEX idx_cmx_user_role_assignment_expire ON cmx_user_role_assignment (effective_until) WHERE status = 1 AND archived = 0;

COMMENT ON TABLE cmx_user_role_assignment IS '用户角色临时授权表';
COMMENT ON COLUMN cmx_user_role_assignment.id IS '主键ID';
COMMENT ON COLUMN cmx_user_role_assignment.user_id IS '用户ID';
COMMENT ON COLUMN cmx_user_role_assignment.role_id IS '角色ID';
COMMENT ON COLUMN cmx_user_role_assignment.effective_from IS '生效开始时间';
COMMENT ON COLUMN cmx_user_role_assignment.effective_until IS '生效结束时间';
COMMENT ON COLUMN cmx_user_role_assignment.reason IS '授权理由（便于审计）';
COMMENT ON COLUMN cmx_user_role_assignment.source IS '授权来源：manual-手动，approval-审批，system-系统';
COMMENT ON COLUMN cmx_user_role_assignment.status IS '状态：0-已撤销，1-生效中';
COMMENT ON COLUMN cmx_user_role_assignment.revoked_by IS '撤销人';
COMMENT ON COLUMN cmx_user_role_assignment.revoked_at IS '撤销时间';
COMMENT ON COLUMN cmx_user_role_assignment.archived IS '归档标志：0-未归档，1-已归档';
COMMENT ON COLUMN cmx_user_role_assignment.create_time IS '创建时间';
COMMENT ON COLUMN cmx_user_role_assignment.update_time IS '更新时间';
COMMENT ON COLUMN cmx_user_role_assignment.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_user_role_assignment.create_name IS '创建人姓名';
COMMENT ON COLUMN cmx_user_role_assignment.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_user_role_assignment.update_name IS '更新人姓名';

-- =====================================================
-- 阶段 2：互斥规则引擎（功能权限互斥 + 角色互斥）
-- 新增表：cmx_exclusion_rule、cmx_exclusion_rule_item
-- 新增权限码：rule:read、rule:manage
-- =====================================================

-- 互斥规则表
DROP TABLE IF EXISTS cmx_exclusion_rule;
CREATE TABLE cmx_exclusion_rule (
    id varchar(64) NOT NULL,
    code varchar(100) NOT NULL,
    name varchar(200) NOT NULL,
    subject_type varchar(20) NOT NULL,
    primary_subject_id varchar(64) NOT NULL,
    violation_message varchar(500),
    priority int4 DEFAULT 0,
    description varchar(500),
    status int4 DEFAULT 1,
    archived int4 DEFAULT 0,
    create_time timestamp DEFAULT CURRENT_TIMESTAMP,
    update_time timestamp DEFAULT CURRENT_TIMESTAMP,
    create_by varchar(100),
    create_name varchar(100),
    update_by varchar(100),
    update_name varchar(100),
    CONSTRAINT pk_cmx_exclusion_rule PRIMARY KEY (id),
    CONSTRAINT uk_cmx_exclusion_rule_code UNIQUE (code)
);

COMMENT ON TABLE cmx_exclusion_rule IS '互斥规则表（功能互斥/角色互斥）';
COMMENT ON COLUMN cmx_exclusion_rule.id IS '主键ID';
COMMENT ON COLUMN cmx_exclusion_rule.code IS '规则编码（唯一）';
COMMENT ON COLUMN cmx_exclusion_rule.name IS '规则名称';
COMMENT ON COLUMN cmx_exclusion_rule.subject_type IS '对象类型：permission-功能权限互斥，role-角色互斥';
COMMENT ON COLUMN cmx_exclusion_rule.primary_subject_id IS '主要对象ID（权限ID或角色ID，取决于 subject_type）';
COMMENT ON COLUMN cmx_exclusion_rule.violation_message IS '违反规则时的错误消息（为空时使用默认消息）';
COMMENT ON COLUMN cmx_exclusion_rule.priority IS '优先级（数字越大越先校验，默认0）';
COMMENT ON COLUMN cmx_exclusion_rule.description IS '描述';
COMMENT ON COLUMN cmx_exclusion_rule.status IS '状态：0-禁用，1-启用';
COMMENT ON COLUMN cmx_exclusion_rule.archived IS '归档标志：0-未归档，1-已归档';
COMMENT ON COLUMN cmx_exclusion_rule.create_time IS '创建时间';
COMMENT ON COLUMN cmx_exclusion_rule.update_time IS '更新时间';
COMMENT ON COLUMN cmx_exclusion_rule.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_exclusion_rule.create_name IS '创建人姓名';
COMMENT ON COLUMN cmx_exclusion_rule.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_exclusion_rule.update_name IS '更新人姓名';

-- 互斥对象明细表
DROP TABLE IF EXISTS cmx_exclusion_rule_item;
CREATE TABLE cmx_exclusion_rule_item (
    id varchar(64) NOT NULL,
    rule_id varchar(64) NOT NULL,
    subject_id varchar(64) NOT NULL,
    CONSTRAINT pk_cmx_exclusion_rule_item PRIMARY KEY (id),
    CONSTRAINT uk_cmx_exclusion_rule_item UNIQUE (rule_id, subject_id)
);

CREATE INDEX idx_cmx_exclusion_rule_item_rule ON cmx_exclusion_rule_item (rule_id);
CREATE INDEX idx_cmx_exclusion_rule_item_subject ON cmx_exclusion_rule_item (subject_id);

COMMENT ON TABLE cmx_exclusion_rule_item IS '互斥对象明细表';
COMMENT ON COLUMN cmx_exclusion_rule_item.id IS '主键ID';
COMMENT ON COLUMN cmx_exclusion_rule_item.rule_id IS '关联规则ID';
COMMENT ON COLUMN cmx_exclusion_rule_item.subject_id IS '互斥对象ID（权限ID或角色ID，与规则 subject_type 一致）';

-- -- 新增权限码（规则管理）
-- INSERT INTO cmx_permission (id, code, name, resource_type, sort_order, status, description) VALUES
-- ('1898765432100002031', 'rule:read',   '查看权限规则', 'api', 31, 1, '查询互斥规则及规则项'),
-- ('1898765432100002032', 'rule:manage', '管理权限规则', 'api', 32, 1, '创建/更新/删除/启用禁用规则及规则项')
-- ON CONFLICT (code) DO NOTHING;
--
-- -- 新增权限码对 admin 角色的批量授权（复用 CTE 逻辑）
-- WITH new_perms AS (
--     SELECT id FROM cmx_permission WHERE code IN ('rule:read', 'rule:manage')
-- )
-- INSERT INTO cmx_role_permission (id, role_id, permission_id)
-- SELECT CONCAT('1898765432100003', LPAD(ROW_NUMBER() OVER ()::TEXT, 4, '0')),
--        '1898765432100001001',
--        id
-- FROM new_perms
ON CONFLICT DO NOTHING;
