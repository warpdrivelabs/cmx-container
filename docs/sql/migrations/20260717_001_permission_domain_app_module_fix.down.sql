-- 回滚：把 cmx_permission 的 app_code / module_code 改回旧的拼接格式。
-- （仅用于回退本迁移；init_ddl/init_dml 已更新为正确短格式，正常不应执行此 down。）

UPDATE cmx_permission
SET app_code = 'fi_cmxfico', module_code = 'fi_cmxfico_gl'
WHERE domain_code = 'fi' AND app_code = 'cmxfico' AND module_code = 'gl';
