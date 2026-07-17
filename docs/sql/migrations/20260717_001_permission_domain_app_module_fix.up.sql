-- 修正 cmx_permission 的 app_code / module_code 命名格式。
--
-- 背景：init_dml.sql 的 cmx_permission 种子数据误用了 cmx_module 表的 code 拼接格式
-- （app_code='fi_cmxfico'、module_code='fi_cmxfico_gl'），与菜单 / 权限的业务短格式不一致
-- （正确应为 app_code='cmxfico'、module_code='gl'）。导致菜单管理页按 DAM 短格式
-- （domain=fi, app=cmxfico, module=gl）查询权限时返回 0 条。
--
-- 本迁移把已入库的错误值批量改回业务短格式（domain_code 原本正确，不动）。
-- 幂等：WHERE 限定旧值，重跑无副作用。

UPDATE cmx_permission
SET app_code = 'cmxfico', module_code = 'gl'
WHERE domain_code = 'fi' AND app_code = 'fi_cmxfico' AND module_code = 'fi_cmxfico_gl';
