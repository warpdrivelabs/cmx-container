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

COMMENT ON TABLE cmx_user IS '用户表';
COMMENT ON COLUMN cmx_user.id IS '主键ID';
COMMENT ON COLUMN cmx_user.username IS '用户名（唯一）';
COMMENT ON COLUMN cmx_user.password_hash IS '密码哈希（Argon2）';
COMMENT ON COLUMN cmx_user.nickname IS '昵称';
COMMENT ON COLUMN cmx_user.email IS '邮箱';
COMMENT ON COLUMN cmx_user.phone IS '手机号';
COMMENT ON COLUMN cmx_user.avatar IS '头像URL';
COMMENT ON COLUMN cmx_user.org_id IS '所属组织ID';
COMMENT ON COLUMN cmx_user.status IS '状态：0-禁用，1-启用';
COMMENT ON COLUMN cmx_user.last_login_at IS '最后登录时间';
COMMENT ON COLUMN cmx_user.last_login_ip IS '最后登录IP';
COMMENT ON COLUMN cmx_user.description IS '描述';
COMMENT ON COLUMN cmx_user.archived IS '是否归档：0-否，1-是';
COMMENT ON COLUMN cmx_user.create_time IS '创建时间';
COMMENT ON COLUMN cmx_user.update_time IS '更新时间';

-- 角色表
CREATE TABLE cmx_role (
    id varchar(64) NOT NULL,
    code varchar(100) NOT NULL,
    name varchar(200) NOT NULL,
    description varchar(500),
    status int4 DEFAULT 1,
    archived int4 DEFAULT 0,
    create_time timestamp DEFAULT CURRENT_TIMESTAMP,
    update_time timestamp DEFAULT CURRENT_TIMESTAMP,
    create_by varchar(100),
    create_name varchar(100),
    update_by varchar(100),
    update_name varchar(100),
    CONSTRAINT pk_cmx_role PRIMARY KEY (id),
    CONSTRAINT uk_cmx_role_code UNIQUE (code)
);

COMMENT ON TABLE cmx_role IS '角色表';
COMMENT ON COLUMN cmx_role.id IS '主键ID';
COMMENT ON COLUMN cmx_role.code IS '角色编码（唯一）';
COMMENT ON COLUMN cmx_role.name IS '角色名称';
COMMENT ON COLUMN cmx_role.description IS '描述';
COMMENT ON COLUMN cmx_role.status IS '状态：0-禁用，1-启用';
COMMENT ON COLUMN cmx_role.archived IS '是否归档：0-否，1-是';

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
    CONSTRAINT pk_cmx_user_role PRIMARY KEY (id)
);

CREATE INDEX idx_cmx_user_role_user ON cmx_user_role (user_id);
CREATE INDEX idx_cmx_user_role_role ON cmx_user_role (role_id);

COMMENT ON TABLE cmx_user_role IS '用户角色关联表';
COMMENT ON COLUMN cmx_user_role.id IS '主键ID';
COMMENT ON COLUMN cmx_user_role.user_id IS '用户ID';
COMMENT ON COLUMN cmx_user_role.role_id IS '角色ID';

-- 权限表
CREATE TABLE cmx_permission (
    id varchar(64) NOT NULL,
    code varchar(200) NOT NULL,
    name varchar(200) NOT NULL,
    type varchar(20) DEFAULT 'api',
    description varchar(500),
    status int4 DEFAULT 1,
    archived int4 DEFAULT 0,
    create_time timestamp DEFAULT CURRENT_TIMESTAMP,
    update_time timestamp DEFAULT CURRENT_TIMESTAMP,
    create_by varchar(100),
    create_name varchar(100),
    update_by varchar(100),
    update_name varchar(100),
    CONSTRAINT pk_cmx_permission PRIMARY KEY (id),
    CONSTRAINT uk_cmx_permission_code UNIQUE (code)
);

COMMENT ON TABLE cmx_permission IS '权限表';
COMMENT ON COLUMN cmx_permission.id IS '主键ID';
COMMENT ON COLUMN cmx_permission.code IS '权限编码（唯一，如 system:user:list）';
COMMENT ON COLUMN cmx_permission.name IS '权限名称';
COMMENT ON COLUMN cmx_permission.type IS '权限类型：api/menu/button';
COMMENT ON COLUMN cmx_permission.description IS '描述';
COMMENT ON COLUMN cmx_permission.status IS '状态：0-禁用，1-启用';

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
    CONSTRAINT pk_cmx_role_permission PRIMARY KEY (id)
);

CREATE INDEX idx_cmx_role_permission_role ON cmx_role_permission (role_id);
CREATE INDEX idx_cmx_role_permission_permission ON cmx_role_permission (permission_id);

COMMENT ON TABLE cmx_role_permission IS '角色权限关联表';
COMMENT ON COLUMN cmx_role_permission.id IS '主键ID';
COMMENT ON COLUMN cmx_role_permission.role_id IS '角色ID';
COMMENT ON COLUMN cmx_role_permission.permission_id IS '权限ID';

-- =====================================================
-- 种子数据：admin 角色和超管权限
-- =====================================================

-- admin 角色
INSERT INTO cmx_role (id, code, name, description, status)
VALUES ('1898765432100001001', 'admin', '系统管理员', '拥有所有权限', 1)
ON CONFLICT (code) DO NOTHING;

-- system:all 权限（超管权限）
INSERT INTO cmx_permission (id, code, name, type, description, status)
VALUES ('1898765432100002001', 'system:all', '所有权限', 'api', '超级管理员拥有的所有权限', 1)
ON CONFLICT (code) DO NOTHING;

-- admin 角色关联 system:all 权限
INSERT INTO cmx_role_permission (id, role_id, permission_id)
VALUES ('1898765432100003001', '1898765432100001001', '1898765432100002001')
ON CONFLICT DO NOTHING;
