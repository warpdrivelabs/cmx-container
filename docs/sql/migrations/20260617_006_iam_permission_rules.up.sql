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
-- 阶段 2：权限互斥与依赖规则（SoD）
-- 新增表：cmx_permission_rule、cmx_permission_rule_item
-- 新增权限码：rule:read、rule:manage
-- =====================================================

-- 权限规则表
DROP TABLE IF EXISTS cmx_permission_rule;
CREATE TABLE cmx_permission_rule (
    id varchar(64) NOT NULL,
    code varchar(100) NOT NULL,
    name varchar(200) NOT NULL,
    rule_type varchar(20) NOT NULL,
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
    CONSTRAINT pk_cmx_permission_rule PRIMARY KEY (id),
    CONSTRAINT uk_cmx_permission_rule_code UNIQUE (code)
);

COMMENT ON TABLE cmx_permission_rule IS '权限规则表（互斥/依赖）';
COMMENT ON COLUMN cmx_permission_rule.id IS '主键ID';
COMMENT ON COLUMN cmx_permission_rule.code IS '规则编码（唯一，如 sod_finance）';
COMMENT ON COLUMN cmx_permission_rule.name IS '规则名称';
COMMENT ON COLUMN cmx_permission_rule.rule_type IS '规则类型：mutual_exclusion-互斥，dependency-依赖';
COMMENT ON COLUMN cmx_permission_rule.violation_message IS '违反规则时的错误消息（为空时使用默认消息）';
COMMENT ON COLUMN cmx_permission_rule.priority IS '优先级（数字越大越先校验，默认0）';
COMMENT ON COLUMN cmx_permission_rule.description IS '描述';
COMMENT ON COLUMN cmx_permission_rule.status IS '状态：0-禁用，1-启用';
COMMENT ON COLUMN cmx_permission_rule.archived IS '归档标志：0-未归档，1-已归档';
COMMENT ON COLUMN cmx_permission_rule.create_time IS '创建时间';
COMMENT ON COLUMN cmx_permission_rule.update_time IS '更新时间';
COMMENT ON COLUMN cmx_permission_rule.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_permission_rule.create_name IS '创建人姓名';
COMMENT ON COLUMN cmx_permission_rule.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_permission_rule.update_name IS '更新人姓名';

-- 规则权限项表
DROP TABLE IF EXISTS cmx_permission_rule_item;
CREATE TABLE cmx_permission_rule_item (
    id varchar(64) NOT NULL,
    rule_id varchar(64) NOT NULL,
    group_seq int4 NOT NULL,
    permission_id varchar(64) NOT NULL,
    CONSTRAINT pk_cmx_permission_rule_item PRIMARY KEY (id),
    CONSTRAINT uk_cmx_permission_rule_item UNIQUE (rule_id, group_seq, permission_id)
);

CREATE INDEX idx_cmx_permission_rule_item_rule ON cmx_permission_rule_item (rule_id);
CREATE INDEX idx_cmx_permission_rule_item_perm ON cmx_permission_rule_item (permission_id);

COMMENT ON TABLE cmx_permission_rule_item IS '规则权限项表';
COMMENT ON COLUMN cmx_permission_rule_item.id IS '主键ID';
COMMENT ON COLUMN cmx_permission_rule_item.rule_id IS '关联规则ID';
COMMENT ON COLUMN cmx_permission_rule_item.group_seq IS '组序号：互斥规则下所有项两两互斥；依赖规则中 group_seq=1 为前置权限，group_seq=2 为依赖权限';
COMMENT ON COLUMN cmx_permission_rule_item.permission_id IS '关联权限ID';

-- 新增权限码（规则管理）
INSERT INTO cmx_permission (id, code, name, resource_type, sort_order, status, description) VALUES
('1898765432100002031', 'rule:read',   '查看权限规则', 'api', 31, 1, '查询互斥/依赖规则及规则项'),
('1898765432100002032', 'rule:manage', '管理权限规则', 'api', 32, 1, '创建/更新/删除/启用禁用规则及规则项')
ON CONFLICT (code) DO NOTHING;

-- 新增权限码对 admin 角色的批量授权（复用 CTE 逻辑）
WITH new_perms AS (
    SELECT id FROM cmx_permission WHERE code IN ('rule:read', 'rule:manage')
)
INSERT INTO cmx_role_permission (id, role_id, permission_id)
SELECT CONCAT('1898765432100003', LPAD(ROW_NUMBER() OVER ()::TEXT, 4, '0')),
       '1898765432100001001',
       id
FROM new_perms
ON CONFLICT DO NOTHING;

-- 示例规则种子数据（可选）
INSERT INTO cmx_permission_rule (id, code, name, rule_type, violation_message, priority, status, description) VALUES
('1898765432100004001', 'sod_finance', '财务职责分离规则', 'mutual_exclusion',
 '拥有审计权限的用户不能同时拥有录入权限', 10, 0, '防止财务造假（默认禁用）')
ON CONFLICT (code) DO NOTHING;

INSERT INTO cmx_permission_rule (id, code, name, rule_type, violation_message, priority, status, description) VALUES
('1898765432100004002', 'dep_user_mgmt', '用户管理依赖规则', 'dependency',
 '删除用户前必须能查看用户列表', 5, 0, '确保操作可追溯（默认禁用）')
ON CONFLICT (code) DO NOTHING;

-- =====================================================
-- 阶段 4：角色层级（不含权限继承）
-- 修改表：cmx_role 增加 parent_role_id 字段
-- =====================================================

ALTER TABLE cmx_role ADD COLUMN IF NOT EXISTS parent_role_id varchar(64);
CREATE INDEX IF NOT EXISTS idx_cmx_role_parent ON cmx_role (parent_role_id);
COMMENT ON COLUMN cmx_role.parent_role_id IS '父角色ID（NULL表示根角色，不支持角色权限继承，仅用于层级展示）';
