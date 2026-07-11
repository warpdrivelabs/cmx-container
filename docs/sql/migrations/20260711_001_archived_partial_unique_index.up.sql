-- 唯一索引改为部分唯一索引（WHERE archived = 0）
-- 目的：配合逻辑删除（archived=1），归档记录不再占用唯一键，允许同名数据重新插入
-- 涉及表：cmx_user / cmx_role / cmx_permission / cmx_menu / cmx_user_role / cmx_role_permission

-- 1. cmx_user.username
DROP INDEX IF EXISTS uk_cmx_user_username;
CREATE UNIQUE INDEX uk_cmx_user_username ON cmx_user (username) WHERE archived = 0;

-- 2. cmx_user.email（保留 email IS NOT NULL 条件）
DROP INDEX IF EXISTS uk_cmx_user_email;
CREATE UNIQUE INDEX uk_cmx_user_email ON cmx_user (email) WHERE email IS NOT NULL AND archived = 0;

-- 3. cmx_role.code
DROP INDEX IF EXISTS uk_cmx_role_code;
CREATE UNIQUE INDEX uk_cmx_role_code ON cmx_role (code) WHERE archived = 0;

-- 4. cmx_permission.code
DROP INDEX IF EXISTS uk_cmx_permission_code;
CREATE UNIQUE INDEX uk_cmx_permission_code ON cmx_permission (code) WHERE archived = 0;

-- 5. cmx_menu.code
DROP INDEX IF EXISTS uk_cmx_menu_code;
CREATE UNIQUE INDEX uk_cmx_menu_code ON cmx_menu (code) WHERE archived = 0;

-- 6. cmx_user_role (user_id, role_id)
DROP INDEX IF EXISTS uk_cmx_user_role;
CREATE UNIQUE INDEX uk_cmx_user_role ON cmx_user_role (user_id, role_id) WHERE archived = 0;

-- 7. cmx_role_permission (role_id, permission_id)
DROP INDEX IF EXISTS uk_cmx_role_permission;
CREATE UNIQUE INDEX uk_cmx_role_permission ON cmx_role_permission (role_id, permission_id) WHERE archived = 0;
