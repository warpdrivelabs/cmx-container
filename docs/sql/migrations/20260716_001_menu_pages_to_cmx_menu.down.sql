-- 回滚：清理迁移导入的菜单（按扫描到的 domain/application/module 删除）
DELETE FROM cmx_menu WHERE domain_code = 'fi' AND application_code = 'cmxfico' AND module_code = 'gl';
DELETE FROM cmx_menu WHERE domain_code = 'fi' AND application_code = 'cmxfico' AND module_code = 'report';
