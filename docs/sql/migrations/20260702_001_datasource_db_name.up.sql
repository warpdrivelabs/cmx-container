-- 数据源表新增 db_name 字段（数据源显示名称，便于运维识别）

ALTER TABLE cmx_sys_datasource ADD COLUMN IF NOT EXISTS db_name VARCHAR(128);

COMMENT ON COLUMN cmx_sys_datasource.db_name IS '数据源名称（便于识别的显示名称）';
