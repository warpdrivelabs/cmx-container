-- =====================================================
-- cmx-plugin 数据库表结构 DDL
-- 插件生命周期管理系统 - PostgreSQL 版本
-- =====================================================

-- 1. 插件注册主表
CREATE TABLE IF NOT EXISTS cmx_plugin (
    id              BIGSERIAL PRIMARY KEY,
    plugin_id       VARCHAR(255) NOT NULL UNIQUE,
    name            VARCHAR(255) NOT NULL,
    version         VARCHAR(50) NOT NULL,
    status          VARCHAR(50) NOT NULL DEFAULT 'installed',
    wasm_path       TEXT,
    install_path    TEXT NOT NULL,
    config_path     TEXT,
    db_id           VARCHAR(255) NOT NULL DEFAULT 'default',
    is_system       BOOLEAN NOT NULL DEFAULT FALSE,
    is_locked       BOOLEAN NOT NULL DEFAULT FALSE,
    domain_code     VARCHAR(100),
    application_code VARCHAR(100),
    module_code     VARCHAR(100),
    vendor_name     VARCHAR(255),
    vendor_url      VARCHAR(512),
    vendor_contact  VARCHAR(255),
    metadata        JSONB,
    signature_algorithm VARCHAR(50),
    signer_key_id   VARCHAR(255),
    activated_at    TIMESTAMP,
    created_at      TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at      TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_cmx_plugin_plugin_id ON cmx_plugin(plugin_id);
CREATE INDEX idx_cmx_plugin_status ON cmx_plugin(status);
CREATE INDEX idx_cmx_plugin_db_id ON cmx_plugin(db_id);
CREATE INDEX idx_cmx_plugin_is_system ON cmx_plugin(is_system);

-- 2. 版本历史表
CREATE TABLE IF NOT EXISTS cmx_plugin_versions (
    id              BIGSERIAL PRIMARY KEY,
    plugin_id       VARCHAR(255) NOT NULL,
    version         VARCHAR(50) NOT NULL,
    version_type    VARCHAR(50) NOT NULL DEFAULT 'release',
    from_version    VARCHAR(50),
    install_path    TEXT NOT NULL,
    wasm_path       TEXT,
    backup_path     TEXT,
    is_current      BOOLEAN NOT NULL DEFAULT FALSE,
    installed_at    TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    uninstalled_at  TIMESTAMP,
    installed_by    VARCHAR(255),
    install_reason  TEXT,
    
    CONSTRAINT fk_cmx_plugin_versions_plugin 
        FOREIGN KEY (plugin_id) REFERENCES cmx_plugin(plugin_id) ON DELETE CASCADE
);

CREATE INDEX idx_cmx_plugin_versions_plugin_id ON cmx_plugin_versions(plugin_id);
CREATE INDEX idx_cmx_plugin_versions_version ON cmx_plugin_versions(version);
CREATE INDEX idx_cmx_plugin_versions_is_current ON cmx_plugin_versions(is_current);

-- 3. 依赖关系表
CREATE TABLE IF NOT EXISTS cmx_plugin_dependencies (
    id                  BIGSERIAL PRIMARY KEY,
    plugin_id           VARCHAR(255) NOT NULL,
    dependency_plugin_id VARCHAR(255) NOT NULL,
    version_constraint  VARCHAR(100) NOT NULL,
    is_optional         BOOLEAN NOT NULL DEFAULT FALSE,
    resolved_version    VARCHAR(50),
    
    CONSTRAINT fk_cmx_plugin_dependencies_plugin 
        FOREIGN KEY (plugin_id) REFERENCES cmx_plugin(plugin_id) ON DELETE CASCADE
);

CREATE INDEX idx_cmx_plugin_dependencies_plugin_id ON cmx_plugin_dependencies(plugin_id);
CREATE INDEX idx_cmx_plugin_dependencies_dep_id ON cmx_plugin_dependencies(dependency_plugin_id);

-- 4. 节点部署记录表
CREATE TABLE IF NOT EXISTS cmx_plugin_deployments (
    id              BIGSERIAL PRIMARY KEY,
    plugin_id       VARCHAR(255) NOT NULL,
    node_id         VARCHAR(255) NOT NULL,
    version         VARCHAR(50) NOT NULL,
    status          VARCHAR(50) NOT NULL,
    deployed_at     TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    error_message   TEXT,
    
    CONSTRAINT fk_cmx_plugin_deployments_plugin 
        FOREIGN KEY (plugin_id) REFERENCES cmx_plugin(plugin_id) ON DELETE CASCADE
);

CREATE INDEX idx_cmx_plugin_deployments_plugin_id ON cmx_plugin_deployments(plugin_id);
CREATE INDEX idx_cmx_plugin_deployments_node_id ON cmx_plugin_deployments(node_id);
CREATE INDEX idx_cmx_plugin_deployments_status ON cmx_plugin_deployments(status);

-- 5. 审计日志表
CREATE TABLE IF NOT EXISTS cmx_plugin_audit_log (
    id              BIGSERIAL PRIMARY KEY,
    plugin_id       VARCHAR(255) NOT NULL,
    operation_type  VARCHAR(50) NOT NULL,
    operator        VARCHAR(255) NOT NULL,
    status          VARCHAR(50) NOT NULL,
    details         JSONB,
    error_message   TEXT,
    client_ip       VARCHAR(45),
    user_agent      VARCHAR(512),
    timestamp       TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_cmx_plugin_audit_log_plugin_id ON cmx_plugin_audit_log(plugin_id);
CREATE INDEX idx_cmx_plugin_audit_log_operation_type ON cmx_plugin_audit_log(operation_type);
CREATE INDEX idx_cmx_plugin_audit_log_timestamp ON cmx_plugin_audit_log(timestamp);
CREATE INDEX idx_cmx_plugin_audit_log_plugin_timestamp ON cmx_plugin_audit_log(plugin_id, timestamp DESC);

-- 6. 回滚记录表
CREATE TABLE IF NOT EXISTS cmx_plugin_rollback (
    id              BIGSERIAL PRIMARY KEY,
    operation_id    VARCHAR(255) NOT NULL UNIQUE,
    plugin_id       VARCHAR(255) NOT NULL,
    from_version    VARCHAR(50) NOT NULL,
    to_version      VARCHAR(50) NOT NULL,
    backup_path    TEXT NOT NULL,
    status          VARCHAR(50) NOT NULL,
    created_at      TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    
    CONSTRAINT fk_cmx_plugin_rollback_plugin 
        FOREIGN KEY (plugin_id) REFERENCES cmx_plugin(plugin_id) ON DELETE CASCADE
);

CREATE INDEX idx_cmx_plugin_rollback_plugin_id ON cmx_plugin_rollback(plugin_id);
CREATE INDEX idx_cmx_plugin_rollback_operation_id ON cmx_plugin_rollback(operation_id);

-- 7. 系统默认插件配置表
CREATE TABLE IF NOT EXISTS cmx_system_plugins (
    id              BIGSERIAL PRIMARY KEY,
    plugin_id       VARCHAR(255) NOT NULL UNIQUE,
    version         VARCHAR(50) NOT NULL,
    fallback_version VARCHAR(50),
    install_order   INTEGER NOT NULL DEFAULT 0,
    is_optional     BOOLEAN NOT NULL DEFAULT FALSE,
    retry_count     INTEGER NOT NULL DEFAULT 3,
    source_type     VARCHAR(50) NOT NULL,
    source_path     TEXT NOT NULL,
    created_at      TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at      TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_cmx_system_plugins_install_order ON cmx_system_plugins(install_order);
CREATE INDEX idx_cmx_system_plugins_is_optional ON cmx_system_plugins(is_optional);

-- 8. 节点信息表
CREATE TABLE IF NOT EXISTS cmx_plugin_nodes (
    id              BIGSERIAL PRIMARY KEY,
    node_id         VARCHAR(255) NOT NULL UNIQUE,
    node_name       VARCHAR(255) NOT NULL,
    node_type       VARCHAR(50) NOT NULL DEFAULT 'worker',
    status          VARCHAR(50) NOT NULL DEFAULT 'offline',
    host            VARCHAR(255),
    port            INTEGER,
    capabilities    JSONB,
    metadata        JSONB,
    last_heartbeat TIMESTAMP,
    created_at      TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at      TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_cmx_plugin_nodes_node_id ON cmx_plugin_nodes(node_id);
CREATE INDEX idx_cmx_plugin_nodes_status ON cmx_plugin_nodes(status);
CREATE INDEX idx_cmx_plugin_nodes_node_type ON cmx_plugin_nodes(node_type);

-- =====================================================
-- 触发器：自动更新 updated_at 时间戳
-- =====================================================

-- cmx_plugin 表触发器
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = CURRENT_TIMESTAMP;
    RETURN NEW;
END;
$$ language 'plpgsql';

CREATE TRIGGER update_cmx_plugin_updated_at 
    BEFORE UPDATE ON cmx_plugin 
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_cmx_system_plugins_updated_at 
    BEFORE UPDATE ON cmx_system_plugins 
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_cmx_plugin_nodes_updated_at 
    BEFORE UPDATE ON cmx_plugin_nodes 
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- =====================================================
-- 注释说明
-- =====================================================

COMMENT ON TABLE cmx_plugin IS '插件注册主表，存储所有已安装插件的元数据';
COMMENT ON TABLE cmx_plugin_versions IS '版本历史表，记录每个插件的版本变更';
COMMENT ON TABLE cmx_plugin_dependencies IS '依赖关系表，记录插件之间的依赖';
COMMENT ON TABLE cmx_plugin_deployments IS '节点部署记录表，记录插件在各个节点的部署状态';
COMMENT ON TABLE cmx_plugin_audit_log IS '审计日志表，记录所有插件操作';
COMMENT ON TABLE cmx_plugin_rollback IS '回滚记录表，记录回滚操作信息';
COMMENT ON TABLE cmx_system_plugins IS '系统默认插件配置表';
COMMENT ON TABLE cmx_plugin_nodes IS '节点信息表';
