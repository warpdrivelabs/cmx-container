-- 回滚：删除 db_name 字段
ALTER TABLE cmx_sys_datasource DROP COLUMN IF EXISTS db_name;
