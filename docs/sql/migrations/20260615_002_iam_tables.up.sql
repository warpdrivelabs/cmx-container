-- =====================================================
-- 用户权限角色管理表（cmx-iam）
-- 包含：用户表、角色表、用户角色关联表、权限表、角色权限关联表
-- =====================================================

-- 用户表
CREATE TABLE IF NOT EXISTS cmx_user (
    id varchar(64) NOT NULL,
    username varchar(100) NOT NULL,
    password_hash varchar(500),
    nickname varchar(100),
    email varchar(200),
    phone varchar(50),
    avatar varchar(500),
    org_id varchar(64),
    gender INT4 DEFAULT 0,
    status int4 DEFAULT 1,
    last_login_at timestamp,
    last_login_ip varchar(50),
    description varchar(500),
    archived int4 DEFAULT 0,
    create_time timestamp DEFAULT CURRENT_TIMESTAMP,
    update_time timestamp DEFAULT CURRENT_TIMESTAMP,
    create_by varchar(100),
    create_name varchar(100),
    update_by varchar(100),
    update_name varchar(100),
    PRIMARY KEY (id)
);

CREATE UNIQUE INDEX IF NOT EXISTS uk_cmx_user_username ON cmx_user (username);
CREATE UNIQUE INDEX IF NOT EXISTS uk_cmx_user_email ON cmx_user (email) WHERE email IS NOT NULL;

COMMENT ON TABLE cmx_user IS '用户表';
COMMENT ON COLUMN cmx_user.id IS '主键ID';
COMMENT ON COLUMN cmx_user.username IS '用户名（唯一）';
COMMENT ON COLUMN cmx_user.password_hash IS '密码哈希（Argon2）';
COMMENT ON COLUMN cmx_user.nickname IS '昵称';
COMMENT ON COLUMN cmx_user.email IS '邮箱';
COMMENT ON COLUMN cmx_user.phone IS '手机号';
COMMENT ON COLUMN cmx_user.avatar IS '头像URL';
COMMENT ON COLUMN cmx_user.org_id IS '所属组织ID';
COMMENT ON COLUMN cmx_user.gender IS '性别：0-未知，1-男，2-女';
COMMENT ON COLUMN cmx_user.status IS '状态：0-禁用，1-启用';
COMMENT ON COLUMN cmx_user.last_login_at IS '最后登录时间';
COMMENT ON COLUMN cmx_user.last_login_ip IS '最后登录IP';
COMMENT ON COLUMN cmx_user.description IS '描述';
COMMENT ON COLUMN cmx_user.archived IS '是否归档：0-否，1-是';
COMMENT ON COLUMN cmx_user.create_time IS '创建时间';
COMMENT ON COLUMN cmx_user.update_time IS '更新时间';
-- =============================================
-- 29a. 角色组表 (cmx_role_group)
-- =============================================
DROP TABLE IF EXISTS cmx_role_group;
CREATE TABLE IF NOT EXISTS cmx_role_group
(
    id          VARCHAR(64)  NOT NULL,
    name        VARCHAR(100) NOT NULL,
    parent_id   VARCHAR(64),
    sort_order  INT4      DEFAULT 0,
    description VARCHAR(500),
    status      INT4      DEFAULT 1,
    archived    INT4      DEFAULT 0,
    create_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    update_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    create_by   VARCHAR(100),
    create_name VARCHAR(100),
    update_by   VARCHAR(100),
    update_name VARCHAR(100),
    PRIMARY KEY (id)
);

COMMENT ON TABLE cmx_role_group IS '角色组表（树形结构）';
COMMENT ON COLUMN cmx_role_group.id IS '主键ID';
COMMENT ON COLUMN cmx_role_group.name IS '角色组名称';
COMMENT ON COLUMN cmx_role_group.parent_id IS '父角色组ID（NULL=根节点）';
COMMENT ON COLUMN cmx_role_group.sort_order IS '排序序号';
COMMENT ON COLUMN cmx_role_group.description IS '描述';
COMMENT ON COLUMN cmx_role_group.status IS '状态：0-禁用，1-启用';
COMMENT ON COLUMN cmx_role_group.archived IS '归档标志：0-未归档，1-已归档';
COMMENT ON COLUMN cmx_role_group.create_time IS '创建时间';
COMMENT ON COLUMN cmx_role_group.update_time IS '更新时间';
COMMENT ON COLUMN cmx_role_group.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_role_group.create_name IS '创建人姓名';
COMMENT ON COLUMN cmx_role_group.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_role_group.update_name IS '更新人姓名';

CREATE INDEX IF NOT EXISTS idx_cmx_role_group_parent ON cmx_role_group (parent_id);
CREATE INDEX IF NOT EXISTS idx_cmx_role_group_status ON cmx_role_group (status);

-- =============================================
-- 30. 角色表 (cmx_role)
-- =============================================
DROP TABLE IF EXISTS cmx_role;
CREATE TABLE IF NOT EXISTS cmx_role
(
    id            VARCHAR(64)  NOT NULL,
    code          VARCHAR(100) NOT NULL,
    name          VARCHAR(100) NOT NULL,
    role_group_id VARCHAR(64),
    data_scope    INT4      DEFAULT 1,
    sort_order    INT4      DEFAULT 0,
    description   VARCHAR(500),
    status        INT4      DEFAULT 1,
    archived      INT4      DEFAULT 0,
    create_time    TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    update_time    TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    create_by      VARCHAR(100),
    create_name    VARCHAR(100),
    update_by      VARCHAR(100),
    update_name    VARCHAR(100),
    PRIMARY KEY (id)
);

COMMENT ON TABLE cmx_role IS '角色表';
COMMENT ON COLUMN cmx_role.id IS '主键ID';
COMMENT ON COLUMN cmx_role.code IS '角色编码（唯一）';
COMMENT ON COLUMN cmx_role.name IS '角色名称';
COMMENT ON COLUMN cmx_role.role_group_id IS '所属角色组ID';
COMMENT ON COLUMN cmx_role.data_scope IS '数据权限范围：1-全部，2-自定义，3-本部门，4-本部门及子部门，5-仅本人（预留字段）';
COMMENT ON COLUMN cmx_role.sort_order IS '排序序号';
COMMENT ON COLUMN cmx_role.description IS '描述';
COMMENT ON COLUMN cmx_role.status IS '状态：0-禁用，1-启用';
COMMENT ON COLUMN cmx_role.archived IS '归档标志：0-未归档，1-已归档';
COMMENT ON COLUMN cmx_role.create_time IS '创建时间';
COMMENT ON COLUMN cmx_role.update_time IS '更新时间';
COMMENT ON COLUMN cmx_role.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_role.create_name IS '创建人姓名';
COMMENT ON COLUMN cmx_role.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_role.update_name IS '更新人姓名';

CREATE UNIQUE INDEX IF NOT EXISTS uk_cmx_role_code ON cmx_role (code);
CREATE INDEX IF NOT EXISTS idx_cmx_role_group_id ON cmx_role (role_group_id);



-- 用户角色关联表
CREATE TABLE IF NOT EXISTS cmx_user_role (
    id varchar(64) NOT NULL,
    user_id varchar(64) NOT NULL,
    role_id varchar(64) NOT NULL,
    archived int4 DEFAULT 0,
    create_time timestamp DEFAULT CURRENT_TIMESTAMP,
    update_time timestamp DEFAULT CURRENT_TIMESTAMP,
    create_by varchar(100),
    create_name varchar(100),
    update_by varchar(100),
    update_name varchar(100),
    PRIMARY KEY (id)
);

CREATE UNIQUE INDEX IF NOT EXISTS uk_cmx_user_role ON cmx_user_role (user_id, role_id);
CREATE INDEX IF NOT EXISTS idx_cmx_user_role_user ON cmx_user_role (user_id);
CREATE INDEX IF NOT EXISTS idx_cmx_user_role_role ON cmx_user_role (role_id);

COMMENT ON TABLE cmx_user_role IS '用户角色关联表';
COMMENT ON COLUMN cmx_user_role.id IS '主键ID';
COMMENT ON COLUMN cmx_user_role.user_id IS '用户ID';
COMMENT ON COLUMN cmx_user_role.role_id IS '角色ID';

-- 权限表
CREATE TABLE IF NOT EXISTS cmx_permission
(
    id            VARCHAR(64)  NOT NULL,
    code          VARCHAR(200) NOT NULL,
    name          VARCHAR(100) NOT NULL,
    resource_type VARCHAR(20) DEFAULT 'api',
    parent_id     VARCHAR(64),
    sort_order    INT4      DEFAULT 0,
    description   VARCHAR(500),
    domain_code   VARCHAR(100),
    app_code      VARCHAR(100),
    module_code   VARCHAR(100),
    extension     TEXT,
    parent_code    VARCHAR(200),
    full_code_path VARCHAR(1000) NOT NULL,
    is_leaf        INT4      DEFAULT 0,
    level          INT4      DEFAULT 1,
    status        INT4      DEFAULT 1,
    archived      INT4      DEFAULT 0,
    create_time   TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    update_time   TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    create_by     VARCHAR(100),
    create_name   VARCHAR(100),
    update_by     VARCHAR(100),
    update_name   VARCHAR(100),
    PRIMARY KEY (id)
);

COMMENT ON TABLE cmx_permission IS '权限表';
COMMENT ON COLUMN cmx_permission.id IS '主键ID';
COMMENT ON COLUMN cmx_permission.code IS '权限编码（唯一，如 system:user:list）';
COMMENT ON COLUMN cmx_permission.name IS '权限名称';
COMMENT ON COLUMN cmx_permission.resource_type IS '资源类型：api-接口，menu-菜单，button-按钮';
COMMENT ON COLUMN cmx_permission.parent_id IS '父权限ID（用于权限树结构）';
COMMENT ON COLUMN cmx_permission.sort_order IS '排序序号';
COMMENT ON COLUMN cmx_permission.description IS '描述';
COMMENT ON COLUMN cmx_permission.domain_code IS '所属域编码（如 platform、tenant）';
COMMENT ON COLUMN cmx_permission.app_code IS '所属应用编码（如 user-center、billing）';
COMMENT ON COLUMN cmx_permission.module_code IS '所属模块编码（如 user、order）';
COMMENT ON COLUMN cmx_permission.extension IS '扩展配置（用户自定义 JSON 文本）';
COMMENT ON COLUMN cmx_permission.parent_code IS '父权限编码（根节点为 NULL）';
COMMENT ON COLUMN cmx_permission.full_code_path IS '权限编码全路径（如 /user:list/user:delete）';
COMMENT ON COLUMN cmx_permission.is_leaf IS '是否叶子节点：1-是，0-否';
COMMENT ON COLUMN cmx_permission.level IS '层级深度（根节点为 1）';
COMMENT ON COLUMN cmx_permission.status IS '状态：0-禁用，1-启用';
COMMENT ON COLUMN cmx_permission.archived IS '归档标志：0-未归档，1-已归档';
COMMENT ON COLUMN cmx_permission.create_time IS '创建时间';
COMMENT ON COLUMN cmx_permission.update_time IS '更新时间';
COMMENT ON COLUMN cmx_permission.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_permission.create_name IS '创建人姓名';
COMMENT ON COLUMN cmx_permission.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_permission.update_name IS '更新人姓名';

CREATE UNIQUE INDEX IF NOT EXISTS uk_cmx_permission_code ON cmx_permission (code);
CREATE INDEX IF NOT EXISTS idx_cmx_permission_parent ON cmx_permission (parent_id);
CREATE INDEX IF NOT EXISTS idx_cmx_permission_full_path ON cmx_permission (full_code_path);
CREATE INDEX IF NOT EXISTS idx_cmx_permission_parent_code ON cmx_permission (parent_code);
CREATE INDEX IF NOT EXISTS idx_cmx_permission_domain_code ON cmx_permission (domain_code);
CREATE INDEX IF NOT EXISTS idx_cmx_permission_app_code ON cmx_permission (app_code);
CREATE INDEX IF NOT EXISTS idx_cmx_permission_module_code ON cmx_permission (module_code);

-- 角色权限关联表
CREATE TABLE IF NOT EXISTS cmx_role_permission (
    id varchar(64) NOT NULL,
    role_id varchar(64) NOT NULL,
    permission_id varchar(64) NOT NULL,
    archived int4 DEFAULT 0,
    create_time timestamp DEFAULT CURRENT_TIMESTAMP,
    update_time timestamp DEFAULT CURRENT_TIMESTAMP,
    create_by varchar(100),
    create_name varchar(100),
    update_by varchar(100),
    update_name varchar(100),
    PRIMARY KEY (id)
);

CREATE UNIQUE INDEX IF NOT EXISTS uk_cmx_role_permission ON cmx_role_permission (role_id, permission_id);
CREATE INDEX IF NOT EXISTS idx_cmx_role_permission_role ON cmx_role_permission (role_id);
CREATE INDEX IF NOT EXISTS idx_cmx_role_permission_permission ON cmx_role_permission (permission_id);

COMMENT ON TABLE cmx_role_permission IS '角色权限关联表';
COMMENT ON COLUMN cmx_role_permission.id IS '主键ID';
COMMENT ON COLUMN cmx_role_permission.role_id IS '角色ID';
COMMENT ON COLUMN cmx_role_permission.permission_id IS '权限ID';

-- =====================================================
-- 种子数据：内置角色、权限及关联关系
-- =====================================================

-- 内置角色
INSERT INTO cmx_role (id, code, name, data_scope, sort_order, status, description) VALUES
('1898765432100001001', 'admin', '系统管理员', 1, 1, 1, '拥有全部权限'),
('1898765432100001002', 'user', '普通用户', 5, 2, 1, '仅查看本人数据')
ON CONFLICT (code) DO NOTHING;

-- -- 内置权限（resource:action 规范）

INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848667162255360', 'gl:account', '科目管理', 'menu', null, 3, '会计科目体系维护', 1, 0, '2026-06-25 09:53:36.798272', '2026-06-25 09:53:36.798272', null, null, null, null, 'FIN', 'FI', 'GL', null, null, '/gl:account', 0, 1) ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848676112900096', 'gl:account:add', '科目新增', 'button', '7475848667162255360', 1, '新增会计科目', 1, 0, '2026-06-25 09:53:38.915084', '2026-06-25 09:53:38.915084', null, null, null, null, 'FIN', 'FI', 'GL', null, 'gl:account', '/gl:account/gl:account:add', 1, 2) ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848677895479296', 'gl:account:delete', '科目删除', 'button', '7475848667162255360', 3, '删除末级科目', 1, 0, '2026-06-25 09:53:39.343854', '2026-06-25 09:53:39.343854', null, null, null, null, 'FIN', 'FI', 'GL', null, 'gl:account', '/gl:account/gl:account:delete', 1, 2) ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848676989509632', 'gl:account:edit', '科目编辑', 'button', '7475848667162255360', 2, '修改科目信息', 1, 0, '2026-06-25 09:53:39.129204', '2026-06-25 09:53:39.129204', null, null, null, null, 'FIN', 'FI', 'GL', null, 'gl:account', '/gl:account/gl:account:edit', 1, 2) ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848679703224320', 'gl:account:export', '科目导出', 'button', '7475848667162255360', 5, '导出科目体系', 1, 0, '2026-06-25 09:53:39.776420', '2026-06-25 09:53:39.776420', null, null, null, null, 'FIN', 'FI', 'GL', null, 'gl:account', '/gl:account/gl:account:export', 1, 2) ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848678809837568', 'gl:account:query', '科目查询', 'api', '7475848667162255360', 4, '查询科目树形列表', 1, 0, '2026-06-25 09:53:39.562303', '2026-06-25 09:53:39.562303', null, null, null, null, 'FIN', 'FI', 'GL', null, 'gl:account', '/gl:account/gl:account:query', 1, 2) ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848665702637568', 'gl:dashboard', '总账仪表盘', 'menu', null, 1, '总账模块概览看板', 1, 0, '2026-06-25 09:53:36.449545', '2026-06-25 09:53:36.449545', null, null, null, null, 'FIN', 'FI', 'GL', null, null, '/gl:dashboard', 0, 1) ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848670211514368', 'gl:dashboard:refresh', '数据刷新', 'button', '7475848665702637568', 2, '手动刷新仪表盘统计', 1, 0, '2026-06-25 09:53:37.499114', '2026-06-25 09:53:37.499114', null, null, null, null, 'FIN', 'FI', 'GL', null, 'gl:dashboard', '/gl:dashboard/gl:dashboard:refresh', 1, 2) ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848669469122560', 'gl:dashboard:view', '仪表盘查看', 'api', '7475848665702637568', 1, '查看总账仪表盘数据', 1, 0, '2026-06-25 09:53:37.325539', '2026-06-25 09:53:37.325539', null, null, null, null, 'FIN', 'FI', 'GL', null, 'gl:dashboard', '/gl:dashboard/gl:dashboard:view', 1, 2) ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848667845926912', 'gl:period', '期末处理', 'menu', null, 4, '期末结账与损益结转', 1, 0, '2026-06-25 09:53:36.957909', '2026-06-25 09:53:36.957909', null, null, null, null, 'FIN', 'FI', 'GL', null, null, '/gl:period', 0, 1) ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848680584028160', 'gl:period:close', '期末结账', 'button', '7475848667845926912', 1, '对当前会计期间结账', 1, 0, '2026-06-25 09:53:39.985243', '2026-06-25 09:53:39.985243', null, null, null, null, 'FIN', 'FI', 'GL', null, 'gl:period', '/gl:period/gl:period:close', 1, 2) ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848682249166848', 'gl:period:settle', '损益结转', 'button', '7475848667845926912', 3, '结转本期损益至本年利润', 1, 0, '2026-06-25 09:53:40.383092', '2026-06-25 09:53:40.383092', null, null, null, null, 'FIN', 'FI', 'GL', null, 'gl:period', '/gl:period/gl:period:settle', 1, 2) ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848681427083264', 'gl:period:unclose', '反结账', 'button', '7475848667845926912', 2, '撤销已结账期间', 1, 0, '2026-06-25 09:53:40.186302', '2026-06-25 09:53:40.186302', null, null, null, null, 'FIN', 'FI', 'GL', null, 'gl:period', '/gl:period/gl:period:unclose', 1, 2) ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848668558958592', 'gl:report', '报表中心', 'menu', null, 5, '总账报表查询与导出', 1, 0, '2026-06-25 09:53:37.122063', '2026-06-25 09:53:37.122063', null, null, null, null, 'FIN', 'FI', 'GL', null, null, '/gl:report', 0, 1) ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848683444543488', 'gl:report:balance', '资产负债表', 'api', '7475848668558958592', 1, '生成资产负债表', 1, 0, '2026-06-25 09:53:40.665266', '2026-06-25 09:53:40.665266', null, null, null, null, 'FIN', 'FI', 'GL', null, 'gl:report', '/gl:report/gl:report:balance', 1, 2) ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848685168402432', 'gl:report:cashflow', '现金流量表', 'api', '7475848668558958592', 3, '生成现金流量表', 1, 0, '2026-06-25 09:53:41.078737', '2026-06-25 09:53:41.078737', null, null, null, null, 'FIN', 'FI', 'GL', null, 'gl:report', '/gl:report/gl:report:cashflow', 1, 2) ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848684400844800', 'gl:report:income', '利润表', 'api', '7475848668558958592', 2, '生成利润表', 1, 0, '2026-06-25 09:53:40.893176', '2026-06-25 09:53:40.893176', null, null, null, null, 'FIN', 'FI', 'GL', null, 'gl:report', '/gl:report/gl:report:income', 1, 2) ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848666486972416', 'gl:voucher', '凭证管理', 'menu', null, 2, '会计凭证全生命周期管理', 1, 0, '2026-06-25 09:53:36.636949', '2026-06-25 09:53:36.636949', null, null, null, null, 'FIN', 'FI', 'GL', null, null, '/gl:voucher', 0, 1) ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848670991654912', 'gl:voucher:add', '凭证录入', 'button', '7475848666486972416', 1, '新增会计凭证', 1, 0, '2026-06-25 09:53:37.676977', '2026-06-25 09:53:37.676977', null, null, null, null, 'FIN', 'FI', 'GL', null, 'gl:voucher', '/gl:voucher/gl:voucher:add', 1, 2) ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848673445322752', 'gl:voucher:audit', '凭证审核', 'button', '7475848666486972416', 4, '审核/反审核凭证', 1, 0, '2026-06-25 09:53:38.281608', '2026-06-25 09:53:38.281608', null, null, null, null, 'FIN', 'FI', 'GL', null, 'gl:voucher', '/gl:voucher/gl:voucher:audit', 1, 2) ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848675001409536', 'gl:voucher:delete', '凭证删除', 'button', '7475848666486972416', 6, '删除作废凭证', 1, 0, '2026-06-25 09:53:38.655424', '2026-06-25 09:53:38.655424', null, null, null, null, 'FIN', 'FI', 'GL', null, 'gl:voucher', '/gl:voucher/gl:voucher:delete', 1, 2) ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848672686153728', 'gl:voucher:edit', '凭证修改', 'button', '7475848666486972416', 3, '修改未审核凭证', 1, 0, '2026-06-25 09:53:38.088995', '2026-06-25 09:53:38.088995', null, null, null, null, 'FIN', 'FI', 'GL', null, 'gl:voucher', '/gl:voucher/gl:voucher:edit', 1, 2) ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848674095439872', 'gl:voucher:post', '凭证过账', 'button', '7475848666486972416', 5, '将审核凭证过账到账簿', 1, 0, '2026-06-25 09:53:38.439356', '2026-06-25 09:53:38.439356', null, null, null, null, 'FIN', 'FI', 'GL', null, 'gl:voucher', '/gl:voucher/gl:voucher:post', 1, 2) ON CONFLICT (id) DO NOTHING;
INSERT INTO cmx_permission (id, code, name, resource_type, parent_id, sort_order, description, status, archived, create_time, update_time, create_by, create_name, update_by, update_name, domain_code, app_code, module_code, extension, parent_code, full_code_path, is_leaf, level) VALUES ('7475848671801155584', 'gl:voucher:query', '凭证查询', 'api', '7475848666486972416', 2, '按条件查询凭证列表', 1, 0, '2026-06-25 09:53:37.878384', '2026-06-25 09:53:37.878384', null, null, null, null, 'FIN', 'FI', 'GL', null, 'gl:voucher', '/gl:voucher/gl:voucher:query', 1, 2) ON CONFLICT (id) DO NOTHING;



-- INSERT INTO cmx_permission (id, code, name, resource_type, sort_order, status, description) VALUES
-- ('1898765432100002001', 'user:list',        '用户列表',   'api',  1, 1, '查看用户列表'),
-- ('1898765432100002002', 'user:create',      '创建用户',   'api',  2, 1, '创建新用户'),
-- ('1898765432100002003', 'user:read',        '查看用户',   'api',  3, 1, '查看用户详情'),
-- ('1898765432100002004', 'user:update',      '更新用户',   'api',  4, 1, '更新用户信息'),
-- ('1898765432100002005', 'user:delete',      '删除用户',   'api',  5, 1, '删除用户'),
-- ('1898765432100002006', 'user:assign_role', '分配角色',   'api',  6, 1, '为用户分配角色'),
-- ('1898765432100002011', 'role:list',        '角色列表',   'api', 11, 1, '查看角色列表'),
-- ('1898765432100002012', 'role:create',      '创建角色',   'api', 12, 1, '创建新角色'),
-- ('1898765432100002013', 'role:read',        '查看角色',   'api', 13, 1, '查看角色详情'),
-- ('1898765432100002014', 'role:update',      '更新角色',   'api', 14, 1, '更新角色信息'),
-- ('1898765432100002015', 'role:delete',      '删除角色',   'api', 15, 1, '删除角色'),
-- ('1898765432100002016', 'role:assign_perm', '分配权限',   'api', 16, 1, '为角色分配权限'),
-- ('1898765432100002021', 'permission:list',  '权限列表',   'api', 21, 1, '查看权限列表'),
-- ('1898765432100002022', 'permission:read',  '查看权限',   'api', 22, 1, '查看权限详情'),
-- ('1898765432100002023', 'system:all',       '系统管理',   'api', 99, 1, '系统全部权限')
-- ON CONFLICT (code) DO NOTHING;
--
-- -- admin 角色拥有全部权限（使用 CTE + ROW_NUMBER 生成关联 ID）
-- WITH perms AS (
--     SELECT id, ROW_NUMBER() OVER () AS rn FROM cmx_permission
-- )
-- INSERT INTO cmx_role_permission (id, role_id, permission_id)
-- SELECT CONCAT('1898765432100003', LPAD(rn::TEXT, 4, '0')),
--        '1898765432100001001',
--        id
-- FROM perms
-- ON CONFLICT DO NOTHING;
