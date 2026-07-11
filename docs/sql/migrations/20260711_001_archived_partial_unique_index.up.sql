-- 唯一索引改为部分唯一索引（WHERE archived = 0）
-- 目的：配合逻辑删除（archived=1），归档记录不再占用唯一键，允许同名数据重新插入
-- 涉及表：cmx_user / cmx_role / cmx_permission / cmx_menu / cmx_user_role / cmx_role_permission

-- 1. cmx_user.username
DROP INDEX IF EXISTS uk_cmx_user_username;
ALTER TABLE cmx_user
DROP CONSTRAINT IF EXISTS uk_cmx_user_username;
CREATE UNIQUE INDEX uk_cmx_user_username ON cmx_user (username) WHERE archived = 0;

-- 2. cmx_user.email（保留 email IS NOT NULL 条件）
DROP INDEX IF EXISTS uk_cmx_user_email;
ALTER TABLE cmx_user
DROP CONSTRAINT IF EXISTS uk_cmx_user_email;
CREATE UNIQUE INDEX uk_cmx_user_email ON cmx_user (email) WHERE email IS NOT NULL AND archived = 0;

-- 3. cmx_role.code
DROP INDEX IF EXISTS uk_cmx_role_code;

ALTER TABLE cmx_role
DROP CONSTRAINT IF EXISTS uk_cmx_role_code;
CREATE UNIQUE INDEX uk_cmx_role_code ON cmx_role (code) WHERE archived = 0;

-- 4. cmx_permission.code
DROP INDEX IF EXISTS uk_cmx_permission_code;
ALTER TABLE cmx_permission
DROP CONSTRAINT IF EXISTS uk_cmx_permission_code;
CREATE UNIQUE INDEX uk_cmx_permission_code ON cmx_permission (code) WHERE archived = 0;

-- 5. cmx_menu.code
DROP INDEX IF EXISTS uk_cmx_menu_code;
ALTER TABLE cmx_menu
DROP CONSTRAINT IF EXISTS uk_cmx_menu_code;
CREATE UNIQUE INDEX uk_cmx_menu_code ON cmx_menu (code) WHERE archived = 0;

-- 6. cmx_user_role (user_id, role_id)
DROP INDEX IF EXISTS uk_cmx_user_role;
ALTER TABLE cmx_user_role
DROP CONSTRAINT IF EXISTS uk_cmx_user_role;
CREATE UNIQUE INDEX uk_cmx_user_role ON cmx_user_role (user_id, role_id) WHERE archived = 0;

-- 7. cmx_role_permission (role_id, permission_id)
DROP INDEX IF EXISTS uk_cmx_role_permission;
ALTER TABLE cmx_role_permission
DROP CONSTRAINT IF EXISTS uk_cmx_role_permission;
CREATE UNIQUE INDEX uk_cmx_role_permission ON cmx_role_permission (role_id, permission_id) WHERE archived = 0;

-- cmx_auth_client 唯一索引改为部分唯一索引（WHERE archived = 0）
-- 目的：配合逻辑删除（archived=1），归档记录不再占用 client_id 唯一键，
--       允许同名 client_id 在归档后重新插入。
-- 关联表：cmx_auth_client（软删除，见 oauth2_client_handler.rs:485）

DROP INDEX IF EXISTS uk_cmx_auth_client_client_id;
ALTER TABLE cmx_auth_client
DROP CONSTRAINT IF EXISTS uk_cmx_auth_client_client_id;
CREATE UNIQUE INDEX uk_cmx_auth_client_client_id ON cmx_auth_client (client_id) WHERE archived = 0;

-- cmx_exclusion_rule 唯一索引改为部分唯一索引（WHERE archived = 0）
-- 目的：配合逻辑删除（archived=1），归档记录不再占用 code 唯一键，
--       允许同名 code 在归档后重新插入。
-- 关联表：cmx_exclusion_rule（软删除，见 rule/service.rs:784）

DROP INDEX IF EXISTS uk_cmx_exclusion_rule_code;
ALTER TABLE cmx_exclusion_rule
DROP CONSTRAINT IF EXISTS uk_cmx_exclusion_rule_code;
CREATE UNIQUE INDEX uk_cmx_exclusion_rule_code ON cmx_exclusion_rule (code) WHERE archived = 0;
