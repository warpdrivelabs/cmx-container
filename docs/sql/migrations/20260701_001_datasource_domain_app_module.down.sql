-- 回滚：数据源表移除域应用模块归属与 source_type 字段，恢复 db_id 唯一索引

DROP INDEX IF EXISTS idx_datasource_domain_app_module;

ALTER TABLE cmx_sys_datasource DROP COLUMN IF EXISTS domain_code;
ALTER TABLE cmx_sys_datasource DROP COLUMN IF EXISTS application_code;
ALTER TABLE cmx_sys_datasource DROP COLUMN IF EXISTS module_code;
ALTER TABLE cmx_sys_datasource DROP COLUMN IF EXISTS source_type;
-- 回滚：删除 db_name 字段
ALTER TABLE cmx_sys_datasource DROP COLUMN IF EXISTS db_name;

CREATE UNIQUE INDEX IF NOT EXISTS uk_cmx_datasource_db_id ON cmx_sys_datasource (db_id);
