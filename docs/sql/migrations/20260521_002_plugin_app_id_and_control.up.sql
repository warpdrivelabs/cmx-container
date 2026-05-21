-- =============================================
-- 插件系统多应用隔离(app_id)与控制模式支持
-- =============================================

-- =============================================
-- 1. cmx_plugin 表变更
-- =============================================
ALTER TABLE cmx_plugin
    ADD COLUMN IF NOT EXISTS app_id VARCHAR (64) NOT NULL DEFAULT 'default',
    ADD COLUMN IF NOT EXISTS storage_key VARCHAR (500),
    ADD COLUMN IF NOT EXISTS storage_checksum VARCHAR (128);


DROP INDEX IF EXISTS uk_cmx_plugin_plugin_id;
CREATE UNIQUE INDEX IF NOT EXISTS uk_cmx_plugin_app_plugin ON cmx_plugin(app_id, plugin_id);
CREATE INDEX IF NOT EXISTS idx_plugin_app_id ON cmx_plugin(app_id);

COMMENT
ON COLUMN cmx_plugin.app_id IS '应用隔离标识，用于多租户或多应用场景下的插件隔离';
COMMENT
ON COLUMN cmx_plugin.storage_key IS '存储键，标识插件包在存储系统中的唯一键';
COMMENT
ON COLUMN cmx_plugin.storage_checksum IS '存储校验和，用于验证插件包完整性';

-- =============================================
-- 2. cmx_service_define 表变更
-- =============================================
ALTER TABLE cmx_service_define
    ADD COLUMN IF NOT EXISTS app_id VARCHAR (64) NOT NULL DEFAULT 'default';

DROP INDEX IF EXISTS uk_cmx_service_define_key;
CREATE UNIQUE INDEX IF NOT EXISTS uk_cmx_service_define_app_key ON cmx_service_define(app_id, service_key);
CREATE INDEX IF NOT EXISTS idx_service_define_app_id ON cmx_service_define(app_id);

COMMENT
ON COLUMN cmx_service_define.app_id IS '应用隔离标识，用于多租户或多应用场景下的服务隔离';

-- =============================================
-- 3. cmx_plugin_versions 表变更
-- =============================================
ALTER TABLE cmx_plugin_versions
    ADD COLUMN IF NOT EXISTS app_id VARCHAR (64) NOT NULL DEFAULT 'default';

CREATE INDEX IF NOT EXISTS idx_plugin_versions_app_id ON cmx_plugin_versions(app_id);

COMMENT
ON COLUMN cmx_plugin_versions.app_id IS '应用隔离标识，用于多租户或多应用场景下的版本隔离';

-- 删除旧的唯一约束并添加新的（包含 app_id）

DROP INDEX IF EXISTS uk_cmx_plugin_versions_plugin_version;
drop index idx_version_plugin;
drop index idx_version_current;
drop index uk_cmx_plugin_versions_plugin_version;
alter table cmx_plugin_versions
drop
constraint uk_cmx_plugin_versions_plugin_version;

CREATE UNIQUE INDEX IF NOT EXISTS uk_cmx_plugin_versions_plugin_version ON cmx_plugin_versions(plugin_id, app_id, version);

-- =============================================
-- 4. cmx_plugin_deployments 表变更
-- =============================================
ALTER TABLE cmx_plugin_deployments
    ADD COLUMN IF NOT EXISTS app_id VARCHAR (64) NOT NULL DEFAULT 'default';

CREATE INDEX IF NOT EXISTS idx_deploy_app_id ON cmx_plugin_deployments(app_id);

COMMENT
ON COLUMN cmx_plugin_deployments.app_id IS '应用隔离标识，用于多租户或多应用场景下的部署隔离';

-- =============================================
-- 5. cmx_plugin_dependencies 表变更
-- =============================================
ALTER TABLE cmx_plugin_dependencies
    ADD COLUMN IF NOT EXISTS app_id VARCHAR (64) NOT NULL DEFAULT 'default';

CREATE INDEX IF NOT EXISTS idx_dep_app_id ON cmx_plugin_dependencies(app_id);

COMMENT
ON COLUMN cmx_plugin_dependencies.app_id IS '应用隔离标识，用于多租户或多应用场景下的依赖隔离';

-- =============================================
-- 6. cmx_plugin_features 表变更
-- =============================================
ALTER TABLE cmx_plugin_features
    ADD COLUMN IF NOT EXISTS app_id VARCHAR (64) NOT NULL DEFAULT 'default';

CREATE INDEX IF NOT EXISTS idx_features_app_id ON cmx_plugin_features(app_id);

COMMENT
ON COLUMN cmx_plugin_features.app_id IS '应用隔离标识，用于多租户或多应用场景下的功能隔离';

-- =============================================
-- 7. cmx_service_define_version 表变更
-- =============================================
ALTER TABLE cmx_service_define_version
    ADD COLUMN IF NOT EXISTS app_id VARCHAR (64) NOT NULL DEFAULT 'default';

CREATE INDEX IF NOT EXISTS idx_service_define_version_app_id ON cmx_service_define_version(app_id);

COMMENT
ON COLUMN cmx_service_define_version.app_id IS '应用隔离标识，用于多租户或多应用场景下的服务版本隔离';

-- =============================================
-- 8. cmx_meta_table_define 表变更
-- =============================================
ALTER TABLE cmx_meta_table_define
    ADD COLUMN IF NOT EXISTS app_id VARCHAR (64) NOT NULL DEFAULT 'default',
    ADD COLUMN IF NOT EXISTS ddl_status VARCHAR (20) NOT NULL DEFAULT 'pending';

CREATE INDEX IF NOT EXISTS idx_meta_table_define_app_id ON cmx_meta_table_define(app_id);

COMMENT
ON COLUMN cmx_meta_table_define.app_id IS '应用隔离标识，用于多租户或多应用场景下的元数据隔离';
COMMENT
ON COLUMN cmx_meta_table_define.ddl_status IS 'DDL执行状态: pending(待执行), executing(执行中), completed(已完成), failed(执行失败)';

-- =============================================
-- 9. cmx_meta_table_define_version 表变更
-- =============================================
ALTER TABLE cmx_meta_table_define_version
    ADD COLUMN IF NOT EXISTS app_id VARCHAR (64) NOT NULL DEFAULT 'default';

CREATE INDEX IF NOT EXISTS idx_meta_table_define_version_app_id ON cmx_meta_table_define_version(app_id);

COMMENT
ON COLUMN cmx_meta_table_define_version.app_id IS '应用隔离标识，用于多租户或多应用场景下的元数据版本隔离';

-- =============================================
-- 10. cmx_system_plugins 表变更
-- =============================================
ALTER TABLE cmx_system_plugins
    ADD COLUMN IF NOT EXISTS app_id VARCHAR (64) NOT NULL DEFAULT 'default';

CREATE INDEX IF NOT EXISTS idx_system_plugins_app_id ON cmx_system_plugins(app_id);

COMMENT
ON COLUMN cmx_system_plugins.app_id IS '应用隔离标识，用于多租户或多应用场景下的系统插件隔离';

-- =============================================
-- 11. cmx_plugin_audit_log 表变更
-- =============================================
ALTER TABLE cmx_plugin_audit_log
    ADD COLUMN IF NOT EXISTS app_id VARCHAR (64) NOT NULL DEFAULT 'default';

CREATE INDEX IF NOT EXISTS idx_audit_app_id ON cmx_plugin_audit_log(app_id);

COMMENT
ON COLUMN cmx_plugin_audit_log.app_id IS '应用隔离标识，用于多租户或多应用场景下的审计日志隔离';

-- =============================================
-- 12. cmx_plugin_nodes 表变更
-- =============================================
ALTER TABLE cmx_plugin_nodes
    ADD COLUMN IF NOT EXISTS app_id VARCHAR (64) NOT NULL DEFAULT 'default';

CREATE INDEX IF NOT EXISTS idx_node_app_id ON cmx_plugin_nodes(app_id);

COMMENT
ON COLUMN cmx_plugin_nodes.app_id IS '应用隔离标识，用于多租户或多应用场景下的节点隔离';
