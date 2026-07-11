-- =====================================================
-- cmx-auth 认证模块数据库表
-- 包含：OAuth2客户端、Token记录、API Key、密码历史、JWT密钥
-- =====================================================

-- OAuth2 客户端表
CREATE TABLE IF NOT EXISTS cmx_auth_client (
    id varchar(64) NOT NULL,
    client_id varchar(100) NOT NULL,
    client_name varchar(200) NOT NULL,
    client_secret varchar(500),
    client_type varchar(20) NOT NULL,
    redirect_uris text NOT NULL,
    grant_types varchar(200) NOT NULL,
    allowed_scopes text,
    pkce_required bool DEFAULT true,
    status int4 DEFAULT 1,
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

COMMENT ON TABLE cmx_auth_client IS 'OAuth2 客户端表';
COMMENT ON COLUMN cmx_auth_client.id IS '主键ID';
COMMENT ON COLUMN cmx_auth_client.client_id IS '客户端标识';
COMMENT ON COLUMN cmx_auth_client.client_name IS '客户端名称';
COMMENT ON COLUMN cmx_auth_client.client_secret IS '客户端密钥（哈希存储）';
COMMENT ON COLUMN cmx_auth_client.client_type IS '客户端类型：public/confidential';
COMMENT ON COLUMN cmx_auth_client.redirect_uris IS '回调地址（JSON 数组）';
COMMENT ON COLUMN cmx_auth_client.grant_types IS '允许的授权类型（逗号分隔）';
COMMENT ON COLUMN cmx_auth_client.allowed_scopes IS '允许的 scope（逗号分隔）';
COMMENT ON COLUMN cmx_auth_client.pkce_required IS '是否强制 PKCE';
COMMENT ON COLUMN cmx_auth_client.status IS '状态：0-禁用，1-启用';
COMMENT ON COLUMN cmx_auth_client.description IS '描述';
COMMENT ON COLUMN cmx_auth_client.archived IS '是否归档：0-否，1-是';
COMMENT ON COLUMN cmx_auth_client.create_time IS '创建时间';
COMMENT ON COLUMN cmx_auth_client.update_time IS '更新时间';
COMMENT ON COLUMN cmx_auth_client.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_auth_client.create_name IS '创建人姓名';
COMMENT ON COLUMN cmx_auth_client.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_auth_client.update_name IS '更新人姓名';

CREATE UNIQUE INDEX IF NOT EXISTS uk_cmx_auth_client_client_id ON cmx_auth_client (client_id);

-- API Key 表（服务间调用认证）
CREATE TABLE IF NOT EXISTS cmx_auth_api_key (
    id varchar(64) NOT NULL,
    key_prefix varchar(20) NOT NULL,
    key_hash varchar(255) NOT NULL,
    user_id varchar(64),
    service_name varchar(200),
    scopes text,
    rate_limit int4,
    expires_at timestamp,
    status int4 DEFAULT 1,
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

CREATE INDEX IF NOT EXISTS idx_cmx_auth_api_key_user ON cmx_auth_api_key (user_id);

COMMENT ON TABLE cmx_auth_api_key IS 'API Key 表（服务间调用认证）';
COMMENT ON COLUMN cmx_auth_api_key.id IS '主键ID';
COMMENT ON COLUMN cmx_auth_api_key.key_prefix IS 'Key 前缀（展示识别用）';
COMMENT ON COLUMN cmx_auth_api_key.key_hash IS 'SHA256 哈希（明文仅生成时返回一次）';
COMMENT ON COLUMN cmx_auth_api_key.user_id IS '关联用户ID';
COMMENT ON COLUMN cmx_auth_api_key.service_name IS '关联服务名称';
COMMENT ON COLUMN cmx_auth_api_key.scopes IS '允许的 scope（逗号分隔）';
COMMENT ON COLUMN cmx_auth_api_key.rate_limit IS '速率限制（请求/秒）';
COMMENT ON COLUMN cmx_auth_api_key.expires_at IS '过期时间（NULL=永不过期）';
COMMENT ON COLUMN cmx_auth_api_key.status IS '状态：0-禁用，1-启用';
COMMENT ON COLUMN cmx_auth_api_key.description IS '描述';
COMMENT ON COLUMN cmx_auth_api_key.archived IS '是否归档：0-否，1-是';
COMMENT ON COLUMN cmx_auth_api_key.create_time IS '创建时间';
COMMENT ON COLUMN cmx_auth_api_key.update_time IS '更新时间';

CREATE UNIQUE INDEX IF NOT EXISTS uk_cmx_auth_api_key_prefix ON cmx_auth_api_key (key_prefix);

-- 密码历史表（防止密码重复使用）
CREATE TABLE IF NOT EXISTS cmx_auth_password_history (
    id varchar(64) NOT NULL,
    user_id varchar(64) NOT NULL,
    password_hash varchar(500) NOT NULL,
    archived int4 DEFAULT 0,
    create_time timestamp DEFAULT CURRENT_TIMESTAMP,
    update_time timestamp DEFAULT CURRENT_TIMESTAMP,
    create_by varchar(100),
    create_name varchar(100),
    update_by varchar(100),
    update_name varchar(100),
    PRIMARY KEY (id)
);

CREATE INDEX IF NOT EXISTS idx_cmx_auth_password_history_user ON cmx_auth_password_history (user_id);

COMMENT ON TABLE cmx_auth_password_history IS '密码历史表（防止密码重复使用）';
COMMENT ON COLUMN cmx_auth_password_history.id IS '主键ID';
COMMENT ON COLUMN cmx_auth_password_history.user_id IS '用户ID';
COMMENT ON COLUMN cmx_auth_password_history.password_hash IS '密码哈希';
COMMENT ON COLUMN cmx_auth_password_history.archived IS '是否归档：0-否，1-是';
COMMENT ON COLUMN cmx_auth_password_history.create_time IS '创建时间';
COMMENT ON COLUMN cmx_auth_password_history.update_time IS '更新时间';

-- JWT 密钥表（密钥轮换管理）
CREATE TABLE IF NOT EXISTS cmx_auth_jwt_key (
    id varchar(64) NOT NULL,
    kid varchar(100) NOT NULL,
    algorithm varchar(20) NOT NULL,
    public_key text NOT NULL,
    status int4 DEFAULT 1,
    effective_at timestamp NOT NULL,
    expired_at timestamp,
    archived int4 DEFAULT 0,
    create_time timestamp DEFAULT CURRENT_TIMESTAMP,
    update_time timestamp DEFAULT CURRENT_TIMESTAMP,
    create_by varchar(100),
    create_name varchar(100),
    update_by varchar(100),
    update_name varchar(100),
    PRIMARY KEY (id)
);

COMMENT ON TABLE cmx_auth_jwt_key IS 'JWT 密钥表（密钥轮换管理）';
COMMENT ON COLUMN cmx_auth_jwt_key.id IS '主键ID';
COMMENT ON COLUMN cmx_auth_jwt_key.kid IS '密钥ID（Key ID，写入 JWT Header）';
COMMENT ON COLUMN cmx_auth_jwt_key.algorithm IS '签名算法：RS256/HS256';
COMMENT ON COLUMN cmx_auth_jwt_key.public_key IS '公钥 PEM';
COMMENT ON COLUMN cmx_auth_jwt_key.status IS '状态：0-已失效，1-生效中，2-宽限期（仅验签）';
COMMENT ON COLUMN cmx_auth_jwt_key.effective_at IS '生效时间';
COMMENT ON COLUMN cmx_auth_jwt_key.expired_at IS '失效时间';
COMMENT ON COLUMN cmx_auth_jwt_key.archived IS '是否归档：0-否，1-是';
COMMENT ON COLUMN cmx_auth_jwt_key.create_time IS '创建时间';
COMMENT ON COLUMN cmx_auth_jwt_key.update_time IS '更新时间';

CREATE UNIQUE INDEX IF NOT EXISTS uk_cmx_auth_jwt_key_kid ON cmx_auth_jwt_key (kid);


-- Token 事件审计表
-- 记录 Token 签发/撤销/刷新等关键审计事件
CREATE TABLE IF NOT EXISTS cmx_auth_token_event (
                                                    id varchar(64) NOT NULL,
    event_type varchar(50) NOT NULL,
    user_id varchar(64) NOT NULL,
    jti varchar(100),
    detail varchar(500),
    archived int4 DEFAULT 0,
    create_time timestamp DEFAULT CURRENT_TIMESTAMP,
    update_time timestamp DEFAULT CURRENT_TIMESTAMP,
    create_by varchar(100),
    create_name varchar(100),
    update_by varchar(100),
    update_name varchar(100),
    PRIMARY KEY (id)
    );

CREATE INDEX IF NOT EXISTS idx_cmx_auth_token_event_user ON cmx_auth_token_event (user_id);
CREATE INDEX IF NOT EXISTS idx_cmx_auth_token_event_type ON cmx_auth_token_event (event_type);
CREATE INDEX IF NOT EXISTS idx_cmx_auth_token_event_created ON cmx_auth_token_event (create_time);

COMMENT ON TABLE cmx_auth_token_event IS 'Token 事件审计表（记录签发/撤销/刷新等关键事件）';
COMMENT ON COLUMN cmx_auth_token_event.id IS '主键ID';
COMMENT ON COLUMN cmx_auth_token_event.event_type IS '事件类型：token_issued/token_revoked/token_refreshed/login_success/login_failed/password_changed';
COMMENT ON COLUMN cmx_auth_token_event.user_id IS '用户ID';
COMMENT ON COLUMN cmx_auth_token_event.jti IS 'JWT ID（关联 Token）';
COMMENT ON COLUMN cmx_auth_token_event.detail IS '事件详情';
COMMENT ON COLUMN cmx_auth_token_event.archived IS '归档标志：0-未归档，1-已归档';
COMMENT ON COLUMN cmx_auth_token_event.create_time IS '创建时间';
COMMENT ON COLUMN cmx_auth_token_event.update_time IS '更新时间';
COMMENT ON COLUMN cmx_auth_token_event.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_auth_token_event.create_name IS '创建人姓名';
COMMENT ON COLUMN cmx_auth_token_event.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_auth_token_event.update_name IS '更新人姓名';

