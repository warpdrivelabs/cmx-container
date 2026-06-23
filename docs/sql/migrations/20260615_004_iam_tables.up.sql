-- =====================================================
-- 用户权限角色管理表（cmx-iam）
-- 包含：用户表、角色表、用户角色关联表、权限表、角色权限关联表
-- =====================================================

-- 用户表
CREATE TABLE cmx_user (
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
    CONSTRAINT pk_cmx_user PRIMARY KEY (id),
    CONSTRAINT uk_cmx_user_username UNIQUE (username)
);

CREATE UNIQUE INDEX uk_cmx_user_email ON cmx_user (email) WHERE email IS NOT NULL;

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
CREATE TABLE cmx_role_group
(
    id          VARCHAR(64)  NOT NULL,
    name        VARCHAR(100) NOT NULL,
    parent_id   VARCHAR(64),
    sort_order  INT4      DEFAULT 0,
    description VARCHAR(500),
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
COMMENT ON COLUMN cmx_role_group.archived IS '归档标志：0-未归档，1-已归档';
COMMENT ON COLUMN cmx_role_group.create_time IS '创建时间';
COMMENT ON COLUMN cmx_role_group.update_time IS '更新时间';
COMMENT ON COLUMN cmx_role_group.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_role_group.create_name IS '创建人姓名';
COMMENT ON COLUMN cmx_role_group.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_role_group.update_name IS '更新人姓名';

CREATE INDEX idx_cmx_role_group_parent ON cmx_role_group (parent_id);

-- =============================================
-- 30. 角色表 (cmx_role)
-- =============================================
DROP TABLE IF EXISTS cmx_role;
CREATE TABLE cmx_role
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

CREATE UNIQUE INDEX uk_cmx_role_code ON cmx_role (code);
CREATE INDEX idx_cmx_role_group_id ON cmx_role (role_group_id);



-- 用户角色关联表
CREATE TABLE cmx_user_role (
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
    CONSTRAINT pk_cmx_user_role PRIMARY KEY (id),
    CONSTRAINT uk_cmx_user_role UNIQUE (user_id, role_id)
);

CREATE INDEX idx_cmx_user_role_user ON cmx_user_role (user_id);
CREATE INDEX idx_cmx_user_role_role ON cmx_user_role (role_id);

COMMENT ON TABLE cmx_user_role IS '用户角色关联表';
COMMENT ON COLUMN cmx_user_role.id IS '主键ID';
COMMENT ON COLUMN cmx_user_role.user_id IS '用户ID';
COMMENT ON COLUMN cmx_user_role.role_id IS '角色ID';

-- 权限表
CREATE TABLE cmx_permission
(
    id            VARCHAR(64)  NOT NULL,
    code          VARCHAR(200) NOT NULL,
    name          VARCHAR(100) NOT NULL,
    resource_type VARCHAR(20) DEFAULT '',
    parent_id     VARCHAR(64),
    sort_order    INT4      DEFAULT 0,
    description   VARCHAR(500),
    domain_code   VARCHAR(100),
    app_code      VARCHAR(100),
    module_code   VARCHAR(100),
    extension     TEXT,
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
COMMENT ON COLUMN cmx_permission.status IS '状态：0-禁用，1-启用';
COMMENT ON COLUMN cmx_permission.archived IS '归档标志：0-未归档，1-已归档';
COMMENT ON COLUMN cmx_permission.create_time IS '创建时间';
COMMENT ON COLUMN cmx_permission.update_time IS '更新时间';
COMMENT ON COLUMN cmx_permission.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_permission.create_name IS '创建人姓名';
COMMENT ON COLUMN cmx_permission.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_permission.update_name IS '更新人姓名';

CREATE UNIQUE INDEX uk_cmx_permission_code ON cmx_permission (code);
CREATE INDEX idx_cmx_permission_parent ON cmx_permission (parent_id);
CREATE INDEX idx_cmx_permission_domain_code ON cmx_permission (domain_code);
CREATE INDEX idx_cmx_permission_app_code ON cmx_permission (app_code);
CREATE INDEX idx_cmx_permission_module_code ON cmx_permission (module_code);

-- 角色权限关联表
CREATE TABLE cmx_role_permission (
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
    CONSTRAINT pk_cmx_role_permission PRIMARY KEY (id),
    CONSTRAINT uk_cmx_role_permission UNIQUE (role_id, permission_id)
);

CREATE INDEX idx_cmx_role_permission_role ON cmx_role_permission (role_id);
CREATE INDEX idx_cmx_role_permission_permission ON cmx_role_permission (permission_id);

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

-- 内置权限（resource:action 规范）
INSERT INTO cmx_permission (id, code, name, resource_type, sort_order, status, description) VALUES
('1898765432100002001', 'user:list',        '用户列表',   'api',  1, 1, '查看用户列表'),
('1898765432100002002', 'user:create',      '创建用户',   'api',  2, 1, '创建新用户'),
('1898765432100002003', 'user:read',        '查看用户',   'api',  3, 1, '查看用户详情'),
('1898765432100002004', 'user:update',      '更新用户',   'api',  4, 1, '更新用户信息'),
('1898765432100002005', 'user:delete',      '删除用户',   'api',  5, 1, '删除用户'),
('1898765432100002006', 'user:assign_role', '分配角色',   'api',  6, 1, '为用户分配角色'),
('1898765432100002011', 'role:list',        '角色列表',   'api', 11, 1, '查看角色列表'),
('1898765432100002012', 'role:create',      '创建角色',   'api', 12, 1, '创建新角色'),
('1898765432100002013', 'role:read',        '查看角色',   'api', 13, 1, '查看角色详情'),
('1898765432100002014', 'role:update',      '更新角色',   'api', 14, 1, '更新角色信息'),
('1898765432100002015', 'role:delete',      '删除角色',   'api', 15, 1, '删除角色'),
('1898765432100002016', 'role:assign_perm', '分配权限',   'api', 16, 1, '为角色分配权限'),
('1898765432100002021', 'permission:list',  '权限列表',   'api', 21, 1, '查看权限列表'),
('1898765432100002022', 'permission:read',  '查看权限',   'api', 22, 1, '查看权限详情'),
('1898765432100002023', 'system:all',       '系统管理',   'api', 99, 1, '系统全部权限')
ON CONFLICT (code) DO NOTHING;

-- admin 角色拥有全部权限（使用 CTE + ROW_NUMBER 生成关联 ID）
WITH perms AS (
    SELECT id, ROW_NUMBER() OVER () AS rn FROM cmx_permission
)
INSERT INTO cmx_role_permission (id, role_id, permission_id)
SELECT CONCAT('1898765432100003', LPAD(rn::TEXT, 4, '0')),
       '1898765432100001001',
       id
FROM perms
ON CONFLICT DO NOTHING;
