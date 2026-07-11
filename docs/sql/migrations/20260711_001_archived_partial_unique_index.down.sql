-- 回滚：恢复为不带 archived 条件的完整唯一索引
-- 注意：回滚前需确认无 archived=1 的同名记录，否则 CREATE UNIQUE INDEX 会失败

-- 1. cmx_user.username
DROP INDEX IF EXISTS uk_cmx_user_username;
CREATE UNIQUE INDEX uk_cmx_user_username ON cmx_user (username);

-- 2. cmx_user.email
DROP INDEX IF EXISTS uk_cmx_user_email;
CREATE UNIQUE INDEX uk_cmx_user_email ON cmx_user (email) WHERE email IS NOT NULL;

-- 3. cmx_role.code
DROP INDEX IF EXISTS uk_cmx_role_code;
CREATE UNIQUE INDEX uk_cmx_role_code ON cmx_role (code);

-- 4. cmx_permission.code
DROP INDEX IF EXISTS uk_cmx_permission_code;
CREATE UNIQUE INDEX uk_cmx_permission_code ON cmx_permission (code);

-- 5. cmx_menu.code
DROP INDEX IF EXISTS uk_cmx_menu_code;
CREATE UNIQUE INDEX uk_cmx_menu_code ON cmx_menu (code);

-- 6. cmx_user_role (user_id, role_id)
DROP INDEX IF EXISTS uk_cmx_user_role;
CREATE UNIQUE INDEX uk_cmx_user_role ON cmx_user_role (user_id, role_id);

-- 7. cmx_role_permission (role_id, permission_id)
DROP INDEX IF EXISTS uk_cmx_role_permission;
CREATE UNIQUE INDEX uk_cmx_role_permission ON cmx_role_permission (role_id, permission_id);
-- 回滚：cmx_auth_client 唯一索引恢复为普通唯一索引

DROP INDEX IF EXISTS uk_cmx_auth_client_client_id;
CREATE UNIQUE INDEX uk_cmx_auth_client_client_id ON cmx_auth_client (client_id);

-- 回滚：cmx_exclusion_rule 唯一索引恢复为普通唯一索引

DROP INDEX IF EXISTS uk_cmx_exclusion_rule_code;
CREATE UNIQUE INDEX uk_cmx_exclusion_rule_code ON cmx_exclusion_rule (code);
