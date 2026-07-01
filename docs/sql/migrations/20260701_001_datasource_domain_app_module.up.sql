-- 数据源表新增域应用模块归属与 source_type 字段
-- 并去除 db_id 唯一索引（DB 层不强制唯一，改由配置文件层去重）

ALTER TABLE cmx_sys_datasource ADD COLUMN IF NOT EXISTS domain_code VARCHAR(64);
ALTER TABLE cmx_sys_datasource ADD COLUMN IF NOT EXISTS application_code VARCHAR(64);
ALTER TABLE cmx_sys_datasource ADD COLUMN IF NOT EXISTS module_code VARCHAR(64);
ALTER TABLE cmx_sys_datasource ADD COLUMN IF NOT EXISTS source_type VARCHAR(20);

-- 回填默认值（向后兼容：现有数据无域归属，统一回填 default）
-- UPDATE cmx_sys_datasource SET domain_code = 'default' WHERE domain_code IS NULL;
-- UPDATE cmx_sys_datasource SET application_code = 'default' WHERE application_code IS NULL;
-- UPDATE cmx_sys_datasource SET module_code = 'default' WHERE module_code IS NULL;
-- UPDATE cmx_sys_datasource SET source_type = 'default' WHERE source_type IS NULL;

COMMENT ON COLUMN cmx_sys_datasource.domain_code IS '所属域编码';
COMMENT ON COLUMN cmx_sys_datasource.application_code IS '所属应用编码';
COMMENT ON COLUMN cmx_sys_datasource.module_code IS '所属模块编码';
COMMENT ON COLUMN cmx_sys_datasource.source_type IS '数据源类型：default-默认库，biz-业务库，other-其他';

-- 去除 db_id 唯一索引（db_id 在不同域下可重复，唯一性由配置文件层保证）
DROP INDEX IF EXISTS uk_cmx_datasource_db_id;

-- 新增域应用模块联合索引（load_active_datasources 按域过滤）
CREATE INDEX IF NOT EXISTS idx_datasource_domain_app_module
    ON cmx_sys_datasource (domain_code, application_code, module_code);
